-- Scriptpendra · pruebas de las políticas de seguridad
--
-- Se pega en el SQL Editor de Supabase DESPUÉS de `schema.sql` y se ejecuta. Crea dos usuarios y dos grupos de prueba, intenta
-- cosas que NO deberían poder hacerse y comprueba que fallan, y al final deshace todo (ROLLBACK): no deja nada en la base.
--
-- Si todo está bien, termina con el aviso «TODAS LAS PRUEBAS PASARON». Si algo falla, se corta con un error que dice cuál
-- prueba fue; en ese caso NO usar la base con otras personas hasta corregirlo.

begin;

-- Ayudas ------------------------------------------------------------------------------------------------------------------
create function pg_temp.as_user(u uuid) returns void language plpgsql as $$
begin
  perform set_config('request.jwt.claims', json_build_object('sub', u, 'role', 'authenticated')::text, true);
  perform set_config('role', 'authenticated', true);
end $$;

create function pg_temp.as_anon() returns void language plpgsql as $$
begin
  perform set_config('request.jwt.claims', '{}', true);
  perform set_config('role', 'anon', true);
end $$;

create function pg_temp.as_admin_db() returns void language plpgsql as $$
begin
  perform set_config('role', 'postgres', true);
end $$;

-- Ejecuta `sql` y exige que falle. Si `expected` no es vacío, el error tiene que contener ese texto.
create function pg_temp.must_fail(test_name text, sql text, expected text default '') returns void language plpgsql as $$
begin
  begin
    execute sql;
  exception when others then
    if expected <> '' and position(expected in sqlerrm) = 0 and position(expected in sqlstate) = 0 then
      raise exception 'FALLÓ [%]: falló, pero por otra razón: % (%)', test_name, sqlerrm, sqlstate;
    end if;
    raise notice 'ok   [%]', test_name;
    return;
  end;
  raise exception 'FALLÓ [%]: tendría que haber fallado y no falló', test_name;
end $$;

create function pg_temp.must_equal(test_name text, got anyelement, want anyelement) returns void language plpgsql as $$
begin
  if got is distinct from want then
    raise exception 'FALLÓ [%]: se esperaba % y dio %', test_name, want, got;
  end if;
  raise notice 'ok   [%]', test_name;
end $$;

-- Las ayudas las llaman también `authenticated` y `anon`.
grant execute on function pg_temp.as_user(uuid), pg_temp.as_anon(), pg_temp.as_admin_db(),
  pg_temp.must_fail(text, text, text), pg_temp.must_equal(text, anyelement, anyelement) to public;

-- Datos de prueba ---------------------------------------------------------------------------------------------------------
select pg_temp.as_admin_db();
insert into auth.users (id, instance_id, aud, role, email)
values ('00000000-0000-0000-0000-0000000000a1', '00000000-0000-0000-0000-000000000000', 'authenticated', 'authenticated', 'a@test.invalid'),
       ('00000000-0000-0000-0000-0000000000b2', '00000000-0000-0000-0000-000000000000', 'authenticated', 'authenticated', 'b@test.invalid');

do $$
declare
  a uuid := '00000000-0000-0000-0000-0000000000a1';
  b uuid := '00000000-0000-0000-0000-0000000000b2';
  ga uuid; gb uuid; pa uuid; pb uuid; code text; v bigint; n bigint; r text;
begin
  -- Cada uno crea su grupo y su proyecto.
  perform pg_temp.as_user(a);
  ga := public.create_group('Grupo A');
  pa := public.create_project(ga, 'Corto A');
  v := public.push_section(pa, 'guion', 0, '{"x":1}');
  perform pg_temp.must_equal('push de una sección nueva da la versión 1', v, 1::bigint);

  perform pg_temp.as_user(b);
  gb := public.create_group('Grupo B');
  pb := public.create_project(gb, 'Corto B');

  -- B no ve nada de A ni puede tocarlo.
  perform pg_temp.must_equal('B no ve el grupo de A', (select count(*) from public.groups where id = ga), 0::bigint);
  perform pg_temp.must_equal('B no ve el proyecto de A', (select count(*) from public.projects where id = pa), 0::bigint);
  perform pg_temp.must_equal('B no ve las secciones de A', (select count(*) from public.sections where project_id = pa), 0::bigint);
  perform pg_temp.must_equal('B no ve a los miembros de A', (select count(*) from public.memberships where group_id = ga), 0::bigint);
  perform pg_temp.must_fail('B no puede subir una sección a un proyecto de A',
    format('select public.push_section(%L, ''guion'', 1, ''{}'')', pa), 'no permitido');
  perform pg_temp.must_fail('B no puede crear proyectos en el grupo de A',
    format('select public.create_project(%L, ''X'')', ga), 'no permitido');
  perform pg_temp.must_fail('B no puede crear un código para el grupo de A',
    format('select public.create_invite(%L)', ga), 'no permitido');
  perform pg_temp.must_fail('B no puede reservar imágenes en un proyecto de A',
    format('select public.reserve_image(%L, repeat(''a'', 64), ''png'', 1000)', pa), 'no permitido');
  perform pg_temp.must_fail('B no puede escribir directo en la tabla de secciones',
    format('insert into public.sections (project_id, name, content) values (%L, ''guion'', ''x'')', pb));
  perform pg_temp.must_fail('B no puede meterse en un grupo con un INSERT directo',
    format('insert into public.memberships (group_id, user_id) values (%L, %L)', ga, b));
  perform pg_temp.must_fail('B no puede cambiar la cuota de su grupo',
    format('update public.groups set image_quota_bytes = 1000000000 where id = %L', gb));
  perform pg_temp.must_fail('B no puede leer los códigos de invitación', 'select * from public.invites');
  perform pg_temp.must_fail('B no es administrador', 'select * from public.admin_usage()', 'no permitido');
  perform pg_temp.must_fail('un código inventado no sirve', 'select public.join_group(repeat(''z'', 40))', 'inválido');

  -- Sin sesión no se puede nada.
  perform pg_temp.as_anon();
  perform pg_temp.must_fail('anon no lee grupos', 'select * from public.groups', 'permission denied');
  perform pg_temp.must_fail('anon no lee secciones', 'select * from public.sections', 'permission denied');
  perform pg_temp.must_fail('anon no puede llamar a las funciones', 'select public.create_group(''X'')', 'permission denied');

  -- Presencia: quien no es del grupo no la ve ni la toca.
  perform pg_temp.as_user(a);
  perform public.set_presence(pa, 'Ana', 'Guion', 'escena 3');
  perform pg_temp.as_user(b);
  perform pg_temp.must_fail('B no puede avisar presencia en un proyecto de A',
    format('select public.set_presence(%L, ''B'', ''Guion'', '''')', pa), 'no permitido');
  perform pg_temp.must_fail('B no puede ver la presencia en un proyecto de A',
    format('select * from public.project_presence(%L)', pa), 'no permitido');
  perform pg_temp.must_fail('B no puede leer la tabla de presencia', 'select * from public.presence', 'permission denied');
  perform pg_temp.as_user(a);
  perform pg_temp.must_equal('uno no se ve a sí mismo', (select count(*) from public.project_presence(pa)), 0::bigint);

  -- A invita a B: ahora B ve el proyecto de A.
  perform pg_temp.as_user(a);
  code := public.create_invite(ga);
  perform pg_temp.as_user(b);
  perform pg_temp.must_equal('B entra al grupo de A con el código', public.join_group(code), ga);
  perform pg_temp.must_equal('B ve el proyecto de A después de entrar', (select count(*) from public.projects where id = pa), 1::bigint);
  perform pg_temp.must_equal('B lee la sección de A', (select count(*) from public.sections where project_id = pa), 1::bigint);

  perform pg_temp.must_equal('después de entrar, B ve que A está en la escena 3',
    (select detail from public.project_presence(pa) limit 1), 'escena 3');

  -- Escrituras con versión: se detecta que alguien se adelantó.
  v := public.push_section(pa, 'guion', 1, '{"x":2}');
  perform pg_temp.must_equal('push con la versión correcta sube la versión', v, 2::bigint);
  perform pg_temp.as_user(a);
  perform pg_temp.must_fail('push con una versión vieja es un conflicto',
    format('select public.push_section(%L, ''guion'', 1, ''{"x":3}'')', pa), 'conflicto');
  perform pg_temp.must_fail('nombre de sección inválido', format('select public.push_section(%L, ''../guion'', 0, ''{}'')', pa), 'inválida');
  perform pg_temp.must_fail('contenido demasiado grande',
    format('select public.push_section(%L, ''plantas'', 0, repeat(''x'', 5300000))', pa), 'demasiado grande');

  -- Imágenes: tipo, tamaño, cuota, repetidas y ritmo.
  r := public.reserve_image(pa, repeat('a', 64), 'png', 1000);
  perform pg_temp.must_equal('una imagen nueva se reserva', r, 'ok');
  perform pg_temp.must_equal('la misma imagen no ocupa de nuevo', public.reserve_image(pa, repeat('a', 64), 'png', 1000), 'existe');
  perform pg_temp.must_fail('un tipo de archivo no permitido',
    format('select public.reserve_image(%L, repeat(''b'', 64), ''exe'', 1000)', pa), 'inválida');
  perform pg_temp.must_fail('una imagen de más de 2 MB',
    format('select public.reserve_image(%L, repeat(''c'', 64), ''jpg'', 2097153)', pa), 'demasiado grande');
  perform pg_temp.must_fail('un hash que no es un hash',
    format('select public.reserve_image(%L, ''../../x'', ''jpg'', 1000)', pa), 'inválida');
  perform pg_temp.as_admin_db();
  update public.groups set image_quota_bytes = 1500 where id = ga;
  perform pg_temp.as_user(a);
  perform pg_temp.must_fail('pasarse de la cuota del grupo',
    format('select public.reserve_image(%L, repeat(''d'', 64), ''jpg'', 600)', pa), 'cuota');
  select used_bytes into n from public.group_usage(ga);
  perform pg_temp.must_equal('el uso del grupo suma lo reservado', n, 1000::bigint);

  -- Storage: solo se sube lo que tiene reserva y de quien lo reservó.
  perform pg_temp.must_fail('subir un archivo sin reserva',
    format('insert into storage.objects (bucket_id, name, owner) values (''images'', %L, %L)',
           ga || '/' || pa || '/' || repeat('e', 64) || '.png', a));
  perform pg_temp.must_fail('subir al proyecto de otro grupo',
    format('insert into storage.objects (bucket_id, name, owner) values (''images'', %L, %L)',
           gb || '/' || pb || '/' || repeat('a', 64) || '.png', a));
  insert into storage.objects (bucket_id, name, owner)
    values ('images', ga || '/' || pa || '/' || repeat('a', 64) || '.png', a);
  perform pg_temp.must_equal('subir lo reservado funciona',
    (select count(*) from storage.objects where bucket_id = 'images' and name like ga::text || '/%'), 1::bigint);
  perform pg_temp.as_user('00000000-0000-0000-0000-0000000000b2');
  perform pg_temp.as_admin_db();
  delete from public.memberships where group_id = ga and user_id = b;
  perform pg_temp.as_user(b);
  perform pg_temp.must_equal('quien no es del grupo no lee sus imágenes',
    (select count(*) from storage.objects where bucket_id = 'images' and name like ga::text || '/%'), 0::bigint);
end $$;

reset role;
do $$ begin raise notice 'TODAS LAS PRUEBAS PASARON'; end $$;
rollback;
