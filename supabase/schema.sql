-- Scriptpendra · esquema de Supabase
--
-- Se pega entero en el SQL Editor de Supabase (Dashboard → SQL Editor) y se ejecuta una vez. Se puede volver a ejecutar: no
-- borra datos.
--
-- Modelo de seguridad
--   * Nadie entra sin iniciar sesión: el rol `anon` no tiene acceso a nada.
--   * Cada tabla tiene Row Level Security activada y forzada. Quien inició sesión solo ve y toca lo de los grupos de los que es
--     miembro.
--   * Las escrituras importantes (crear grupo, entrar con un código, subir una sección, reservar una imagen) no se hacen con
--     INSERT/UPDATE directos sino con funciones que validan todo en el servidor: pertenencia, tamaños, cuotas y versión.
--   * Las funciones `security definer` fijan `search_path = ''` y nombran cada objeto con su esquema, para que nadie pueda
--     desviarlas con objetos propios.
--   * Los códigos de invitación se guardan con hash: ni quien administre la base puede leerlos después.
--   * La clave `service_role` NO se usa en el programa ni se comparte. El programa solo lleva la URL y la clave `anon`.
--
-- Después de ejecutar este archivo hay que:
--   1. Authentication → Providers → Email: dejar activada la confirmación por mail.
--   2. Authentication → Rate Limits: dejar los límites de fábrica (o más bajos).
--   3. Hacerse administrador (ver el final del archivo).
--   4. Correr `supabase/tests.sql` para comprobar que las políticas aíslan a los grupos.

-- ------------------------------------------------------------------------------------------------
-- Tablas
-- ------------------------------------------------------------------------------------------------

create table if not exists public.profiles (
  id uuid primary key references auth.users (id) on delete cascade,
  display_name text not null default '' check (char_length(display_name) <= 60),
  created_at timestamptz not null default now()
);

-- Dos personas no pueden tener el mismo nombre (sin distinguir mayúsculas): así nadie se hace pasar por otra.
create unique index if not exists profiles_display_name_unique on public.profiles (lower(display_name)) where display_name <> '';

create table if not exists public.admins (
  user_id uuid primary key references auth.users (id) on delete cascade
);

create table if not exists public.groups (
  id uuid primary key default gen_random_uuid(),
  name text not null check (char_length(btrim(name)) between 1 and 80),
  -- Cuánto pueden ocupar en imágenes todos los proyectos del grupo (50 MB de fábrica).
  image_quota_bytes bigint not null default 52428800 check (image_quota_bytes >= 0),
  created_by uuid not null references auth.users (id) on delete cascade,
  created_at timestamptz not null default now()
);

create table if not exists public.memberships (
  group_id uuid not null references public.groups (id) on delete cascade,
  user_id uuid not null references auth.users (id) on delete cascade,
  role text not null default 'member' check (role in ('owner', 'member')),
  joined_at timestamptz not null default now(),
  primary key (group_id, user_id)
);
create index if not exists memberships_user_idx on public.memberships (user_id);

-- Los códigos se guardan con hash (SHA-256): el código en sí solo lo ve quien lo crea, una vez.
create table if not exists public.invites (
  code_hash text primary key,
  group_id uuid not null references public.groups (id) on delete cascade,
  created_by uuid not null references auth.users (id) on delete cascade,
  expires_at timestamptz not null,
  uses_left int not null check (uses_left >= 0)
);

create table if not exists public.join_attempts (
  user_id uuid not null references auth.users (id) on delete cascade,
  at timestamptz not null default now()
);
create index if not exists join_attempts_idx on public.join_attempts (user_id, at);

create table if not exists public.projects (
  id uuid primary key default gen_random_uuid(),
  group_id uuid not null references public.groups (id) on delete cascade,
  name text not null check (char_length(btrim(name)) between 1 and 120),
  created_by uuid not null references auth.users (id) on delete cascade,
  created_at timestamptz not null default now()
);
create index if not exists projects_group_idx on public.projects (group_id);

-- Una fila por sección del proyecto (guion, plantas…). `version` sube con cada escritura: sirve para detectar que alguien más
-- escribió antes (ver `push_section`).
create table if not exists public.sections (
  project_id uuid not null references public.projects (id) on delete cascade,
  name text not null check (name ~ '^[a-z]{3,20}$'),
  content text not null check (octet_length(content) <= 5242880),
  version bigint not null default 1,
  updated_by uuid references auth.users (id) on delete set null,
  updated_at timestamptz not null default now(),
  primary key (project_id, name)
);
-- El nombre (el que registró el servidor) de la persona que subió la última versión, para avisar «Ana cambió el guion».
alter table public.sections add column if not exists updated_who text not null default '' check (char_length(updated_who) <= 60);

-- Las imágenes de un proyecto: el archivo vive en Storage (bucket privado `images`); esta tabla lleva la cuenta de lo que ocupa
-- cada grupo, y es lo que decide si se puede subir otra.
create table if not exists public.images (
  project_id uuid not null references public.projects (id) on delete cascade,
  group_id uuid not null references public.groups (id) on delete cascade,
  hash text not null check (hash ~ '^[0-9a-f]{64}$'),
  ext text not null check (ext in ('jpg', 'png', 'webp')),
  size bigint not null check (size > 0 and size <= 2097152),
  uploaded_by uuid not null references auth.users (id) on delete cascade,
  created_at timestamptz not null default now(),
  primary key (project_id, hash)
);
create index if not exists images_group_idx on public.images (group_id);

-- Quién está en un proyecto y qué pantalla tiene abierta. Se renueva cada pocos segundos; no guarda historia. Solo se toca con
-- las funciones `set_presence` y `project_presence`.
create table if not exists public.presence (
  project_id uuid not null references public.projects (id) on delete cascade,
  user_id uuid not null references auth.users (id) on delete cascade,
  who text not null default '' check (char_length(who) <= 60),
  screen text not null default '' check (char_length(screen) <= 40),
  detail text not null default '' check (char_length(detail) <= 120),
  updated_at timestamptz not null default now(),
  primary key (project_id, user_id)
);

-- ------------------------------------------------------------------------------------------------
-- Permisos base: el rol `anon` no tiene nada; `authenticated` solo lee (y RLS filtra).
-- ------------------------------------------------------------------------------------------------

revoke all on all tables in schema public from anon, authenticated;
revoke all on all functions in schema public from anon, authenticated, public;
alter default privileges in schema public revoke all on tables from anon, authenticated;
alter default privileges in schema public revoke execute on functions from public, anon, authenticated;

grant select on public.profiles, public.groups, public.memberships, public.projects, public.sections, public.images
  to authenticated;
grant delete on public.memberships to authenticated;

alter table public.profiles enable row level security;
alter table public.admins enable row level security;
alter table public.groups enable row level security;
alter table public.memberships enable row level security;
alter table public.invites enable row level security;
alter table public.join_attempts enable row level security;
alter table public.projects enable row level security;
alter table public.sections enable row level security;
alter table public.images enable row level security;
alter table public.presence enable row level security;
alter table public.profiles force row level security;
alter table public.admins force row level security;
alter table public.groups force row level security;
alter table public.memberships force row level security;
alter table public.invites force row level security;
alter table public.join_attempts force row level security;
alter table public.projects force row level security;
alter table public.sections force row level security;
alter table public.images force row level security;
alter table public.presence force row level security;

-- ------------------------------------------------------------------------------------------------
-- Funciones de ayuda (security definer: evitan que las políticas se llamen a sí mismas)
-- ------------------------------------------------------------------------------------------------

create or replace function public.is_member(gid uuid)
returns boolean language sql stable security definer set search_path = '' as $$
  select exists (
    select 1 from public.memberships m where m.group_id = gid and m.user_id = auth.uid()
  );
$$;

create or replace function public.is_owner(gid uuid)
returns boolean language sql stable security definer set search_path = '' as $$
  select exists (
    select 1 from public.memberships m
    where m.group_id = gid and m.user_id = auth.uid() and m.role = 'owner'
  );
$$;

create or replace function public.is_admin()
returns boolean language sql stable security definer set search_path = '' as $$
  select exists (select 1 from public.admins a where a.user_id = auth.uid());
$$;

create or replace function public.project_group(pid uuid)
returns uuid language sql stable security definer set search_path = '' as $$
  select p.group_id from public.projects p where p.id = pid;
$$;

create or replace function public.shares_group_with(uid uuid)
returns boolean language sql stable security definer set search_path = '' as $$
  select exists (
    select 1 from public.memberships a
    join public.memberships b on a.group_id = b.group_id
    where a.user_id = auth.uid() and b.user_id = uid
  );
$$;

-- ------------------------------------------------------------------------------------------------
-- Políticas de lectura
-- ------------------------------------------------------------------------------------------------

drop policy if exists profiles_read on public.profiles;
create policy profiles_read on public.profiles for select to authenticated
  using (id = auth.uid() or public.shares_group_with(id));
-- El perfil no se escribe directo: el nombre se elige una sola vez con `set_display_name`.
drop policy if exists profiles_insert on public.profiles;
drop policy if exists profiles_update on public.profiles;

drop policy if exists admins_read on public.admins;
create policy admins_read on public.admins for select to authenticated using (user_id = auth.uid());

drop policy if exists groups_read on public.groups;
create policy groups_read on public.groups for select to authenticated using (public.is_member(id));

drop policy if exists memberships_read on public.memberships;
create policy memberships_read on public.memberships for select to authenticated
  using (public.is_member(group_id));
-- Cada uno puede irse de un grupo; quien es dueño puede sacar a otros.
drop policy if exists memberships_delete on public.memberships;
create policy memberships_delete on public.memberships for delete to authenticated
  using (user_id = auth.uid() or public.is_owner(group_id));

drop policy if exists projects_read on public.projects;
create policy projects_read on public.projects for select to authenticated using (public.is_member(group_id));

drop policy if exists sections_read on public.sections;
create policy sections_read on public.sections for select to authenticated
  using (public.is_member(public.project_group(project_id)));

drop policy if exists images_read on public.images;
create policy images_read on public.images for select to authenticated using (public.is_member(group_id));

-- `invites` y `join_attempts` no tienen políticas: nadie las toca directamente.

-- ------------------------------------------------------------------------------------------------
-- Escrituras: solo por funciones que validan en el servidor
-- ------------------------------------------------------------------------------------------------

-- El nombre de la persona, tal como lo registró el servidor (nunca el que diga el programa).
create or replace function public.my_name()
returns text language sql stable security definer set search_path = '' as $$
  select coalesce(nullif((select p.display_name from public.profiles p where p.id = auth.uid()), ''), 'Alguien');
$$;

-- Elige el nombre de la persona. Se puede hacer UNA sola vez y no se puede cambiar después (para que nadie cambie de identidad),
-- tiene que ser único y de 3 a 30 letras, números, espacios, puntos, guiones o guiones bajos.
create or replace function public.set_display_name(new_name text)
returns text language plpgsql security definer set search_path = '' as $$
declare
  uid uuid := auth.uid();
  n text := btrim(coalesce(new_name, ''));
  current_name text;
begin
  if uid is null then raise exception 'sin sesión' using errcode = '28000'; end if;
  if char_length(n) not between 3 and 30
     or n !~ '^[A-Za-zÁÉÍÓÚÜÑáéíóúüñ0-9][A-Za-zÁÉÍÓÚÜÑáéíóúüñ0-9 ._-]*[A-Za-zÁÉÍÓÚÜÑáéíóúüñ0-9]$'
     or n ~ '  ' then
    raise exception 'nombre inválido' using errcode = '22023';
  end if;
  if lower(n) in ('admin', 'administrador', 'soporte', 'sistema', 'scriptpendra', 'moderador', 'alguien') then
    raise exception 'nombre reservado' using errcode = '22023';
  end if;
  insert into public.profiles (id) values (uid) on conflict do nothing;
  select p.display_name into current_name from public.profiles p where p.id = uid for update;
  if current_name <> '' then raise exception 'el nombre ya está elegido' using errcode = '42501'; end if;
  update public.profiles set display_name = n where id = uid;  -- si ya lo usa otra persona: error 23505
  return n;
end $$;

-- Aunque alguien lograra escribir en la tabla, un nombre ya elegido no cambia (solo quien administra, desde el SQL Editor).
create or replace function public.keep_display_name()
returns trigger language plpgsql set search_path = '' as $$
begin
  if auth.uid() is not null and old.display_name <> '' and new.display_name is distinct from old.display_name then
    raise exception 'el nombre ya está elegido' using errcode = '42501';
  end if;
  return new;
end $$;
drop trigger if exists keep_display_name on public.profiles;
create trigger keep_display_name before update on public.profiles
  for each row execute function public.keep_display_name();

-- Crea un grupo y deja a quien lo crea como dueño. Máximo 5 grupos por persona.
create or replace function public.create_group(group_name text)
returns uuid language plpgsql security definer set search_path = '' as $$
declare
  uid uuid := auth.uid();
  gid uuid;
begin
  if uid is null then raise exception 'sin sesión' using errcode = '28000'; end if;
  if group_name is null or char_length(btrim(group_name)) not between 1 and 80 then
    raise exception 'nombre inválido' using errcode = '22023';
  end if;
  if (select count(*) from public.memberships m where m.user_id = uid and m.role = 'owner') >= 5 then
    raise exception 'demasiados grupos' using errcode = '53400';
  end if;
  -- Tope de grupos en total, para que una cuenta nueva (el registro es abierto) no llene la base del plan gratuito.
  if (select count(*) from public.groups) >= 300 then
    raise exception 'la nube llegó al máximo de grupos' using errcode = '53400';
  end if;
  insert into public.groups (name, created_by) values (btrim(group_name), uid) returning id into gid;
  insert into public.memberships (group_id, user_id, role) values (gid, uid, 'owner');
  return gid;
end $$;

-- Crea un código de invitación (vale 7 días y 20 usos) y lo devuelve una sola vez.
create or replace function public.create_invite(gid uuid)
returns text language plpgsql security definer set search_path = '' as $$
declare
  code text := encode(extensions.gen_random_bytes(20), 'hex');
begin
  if not public.is_member(gid) then raise exception 'no permitido' using errcode = '42501'; end if;
  insert into public.invites (code_hash, group_id, created_by, expires_at, uses_left)
  values (encode(extensions.digest(code, 'sha256'), 'hex'), gid, auth.uid(), now() + interval '7 days', 20);
  return code;
end $$;

-- Entra a un grupo con un código. Máximo 10 intentos cada 10 minutos por persona.
create or replace function public.join_group(code text)
returns uuid language plpgsql security definer set search_path = '' as $$
declare
  uid uuid := auth.uid();
  inv public.invites%rowtype;
begin
  if uid is null then raise exception 'sin sesión' using errcode = '28000'; end if;
  if code is null or char_length(code) not between 20 and 80 then
    raise exception 'código inválido' using errcode = '22023';
  end if;
  if (select count(*) from public.join_attempts a where a.user_id = uid and a.at > now() - interval '10 minutes') >= 10 then
    raise exception 'demasiados intentos' using errcode = '53400';
  end if;
  -- Los intentos viejos se borran: la tabla no crece sin límite.
  delete from public.join_attempts a where a.at < now() - interval '1 day';
  insert into public.join_attempts (user_id) values (uid);
  select * into inv from public.invites i
  where i.code_hash = encode(extensions.digest(code, 'sha256'), 'hex')
    and i.expires_at > now() and i.uses_left > 0
  for update;
  if not found then raise exception 'código inválido o vencido' using errcode = '22023'; end if;
  update public.invites set uses_left = uses_left - 1 where code_hash = inv.code_hash;
  insert into public.memberships (group_id, user_id) values (inv.group_id, uid) on conflict do nothing;
  return inv.group_id;
end $$;

-- Salir de un grupo. Si queda alguien y nadie es dueño, el miembro más antiguo pasa a serlo. Si no queda nadie, el grupo se borra
-- con todo lo que tiene (proyectos, secciones y la lista de imágenes). Los archivos de imágenes viven en Storage y SQL no los
-- puede borrar: el programa los quita antes de salir, cuando es la última persona.
create or replace function public.leave_group(gid uuid)
returns void language plpgsql security definer set search_path = '' as $$
declare
  uid uuid := auth.uid();
begin
  if uid is null then raise exception 'sin sesión' using errcode = '28000'; end if;
  if not public.is_member(gid) then raise exception 'no permitido' using errcode = '42501'; end if;
  -- Se bloquea el grupo: si salen las dos últimas personas a la vez, una espera a la otra y el grupo se borra una sola vez.
  perform 1 from public.groups g where g.id = gid for update;
  delete from public.presence p
    where p.user_id = uid and p.project_id in (select pr.id from public.projects pr where pr.group_id = gid);
  delete from public.memberships m where m.group_id = gid and m.user_id = uid;
  if not exists (select 1 from public.memberships m where m.group_id = gid) then
    delete from public.groups g where g.id = gid;
  elsif not exists (select 1 from public.memberships m where m.group_id = gid and m.role = 'owner') then
    update public.memberships m set role = 'owner'
    where m.group_id = gid
      and m.user_id = (select m2.user_id from public.memberships m2 where m2.group_id = gid order by m2.joined_at limit 1);
  end if;
end $$;

-- Crea un proyecto en un grupo (máximo 30 por grupo).
create or replace function public.create_project(gid uuid, project_name text)
returns uuid language plpgsql security definer set search_path = '' as $$
declare
  pid uuid;
begin
  if not public.is_member(gid) then raise exception 'no permitido' using errcode = '42501'; end if;
  if project_name is null or char_length(btrim(project_name)) not between 1 and 120 then
    raise exception 'nombre inválido' using errcode = '22023';
  end if;
  if (select count(*) from public.projects p where p.group_id = gid) >= 30 then
    raise exception 'demasiados proyectos' using errcode = '53400';
  end if;
  insert into public.projects (group_id, name, created_by) values (gid, btrim(project_name), auth.uid())
  returning id into pid;
  return pid;
end $$;

-- Sube una sección solo si nadie escribió desde la versión que se leyó (`base_version`; 0 si la sección es nueva). Si alguien
-- se adelantó devuelve el error 40001 y el programa trae lo nuevo, combina y vuelve a intentar. Devuelve la versión nueva.
drop function if exists public.push_section(uuid, text, bigint, text);
create or replace function public.push_section(pid uuid, section text, base_version bigint, new_content text, p_who text default '')
returns bigint language plpgsql security definer set search_path = '' as $$
declare
  current_version bigint;
  next_version bigint;
begin
  if not public.is_member(public.project_group(pid)) then
    raise exception 'no permitido' using errcode = '42501';
  end if;
  if section is null or section !~ '^[a-z]{3,20}$' then raise exception 'sección inválida' using errcode = '22023'; end if;
  if new_content is null or octet_length(new_content) > 5242880 then
    raise exception 'contenido demasiado grande' using errcode = '54000';
  end if;
  select s.version into current_version from public.sections s where s.project_id = pid and s.name = section for update;
  if not found then
    if base_version <> 0 then raise exception 'conflicto' using errcode = '40001'; end if;
    -- Tope de secciones por proyecto (el programa usa 12): sin esto, una cuenta podría llenar la base con nombres inventados.
    if (select count(*) from public.sections s where s.project_id = pid) >= 20 then
      raise exception 'demasiadas secciones' using errcode = '53400';
    end if;
    insert into public.sections (project_id, name, content, version, updated_by, updated_who)
    values (pid, section, new_content, 1, auth.uid(), public.my_name());
    return 1;
  end if;
  if current_version <> base_version then raise exception 'conflicto' using errcode = '40001'; end if;
  next_version := current_version + 1;
  update public.sections
  set content = new_content, version = next_version, updated_by = auth.uid(), updated_at = now(),
      updated_who = public.my_name()
  where project_id = pid and name = section;
  return next_version;
end $$;

-- Reserva lugar para una imagen: valida pertenencia, tipo, tamaño (máx. 2 MB), cuota del grupo y ritmo de subida (máx. 60 por
-- hora y persona). Si la imagen ya está en el proyecto no ocupa de nuevo. Devuelve 'ok' o 'existe'. Recién después de esto el
-- programa puede subir el archivo: Storage solo lo acepta si hay una reserva (ver las políticas de abajo).
create or replace function public.reserve_image(pid uuid, image_hash text, image_ext text, image_size bigint)
returns text language plpgsql security definer set search_path = '' as $$
declare
  gid uuid := public.project_group(pid);
  used bigint;
  quota bigint;
begin
  if gid is null or not public.is_member(gid) then raise exception 'no permitido' using errcode = '42501'; end if;
  if image_hash !~ '^[0-9a-f]{64}$' or image_ext not in ('jpg', 'png', 'webp') then
    raise exception 'imagen inválida' using errcode = '22023';
  end if;
  if image_size is null or image_size <= 0 or image_size > 2097152 then
    raise exception 'imagen demasiado grande' using errcode = '54000';
  end if;
  if exists (select 1 from public.images i where i.project_id = pid and i.hash = image_hash) then return 'existe'; end if;
  if (select count(*) from public.images i where i.uploaded_by = auth.uid() and i.created_at > now() - interval '1 hour') >= 60 then
    raise exception 'demasiadas imágenes seguidas' using errcode = '53400';
  end if;
  select coalesce(sum(i.size), 0) into used from public.images i where i.group_id = gid;
  select g.image_quota_bytes into quota from public.groups g where g.id = gid;
  if used + image_size > quota then raise exception 'cuota de imágenes llena' using errcode = '53100'; end if;
  insert into public.images (project_id, group_id, hash, ext, size, uploaded_by)
  values (pid, gid, image_hash, image_ext, image_size, auth.uid());
  return 'ok';
end $$;

-- Libera una imagen (la quita de la cuenta). El archivo en Storage lo borra el programa después, con su permiso de borrar.
create or replace function public.release_image(pid uuid, image_hash text)
returns void language plpgsql security definer set search_path = '' as $$
begin
  if not public.is_member(public.project_group(pid)) then raise exception 'no permitido' using errcode = '42501'; end if;
  delete from public.images i where i.project_id = pid and i.hash = image_hash;
end $$;

-- Cuánto ocupa un grupo y cuánto puede.
create or replace function public.group_usage(gid uuid)
returns table (used_bytes bigint, quota_bytes bigint, image_count bigint)
language plpgsql stable security definer set search_path = '' as $$
begin
  if not public.is_member(gid) then raise exception 'no permitido' using errcode = '42501'; end if;
  return query
    select coalesce(sum(i.size), 0)::bigint, (select g.image_quota_bytes from public.groups g where g.id = gid),
           count(i.hash)::bigint
    from public.images i where i.group_id = gid;
end $$;

-- Administración (solo quienes están en `admins`): uso de todos los grupos y cuota de cada uno.
create or replace function public.admin_usage()
returns table (group_id uuid, group_name text, members bigint, used_bytes bigint, quota_bytes bigint)
language plpgsql stable security definer set search_path = '' as $$
begin
  if not public.is_admin() then raise exception 'no permitido' using errcode = '42501'; end if;
  return query
    select g.id, g.name, (select count(*) from public.memberships m where m.group_id = g.id),
           coalesce((select sum(i.size) from public.images i where i.group_id = g.id), 0)::bigint, g.image_quota_bytes
    from public.groups g order by 4 desc;
end $$;

create or replace function public.admin_set_quota(gid uuid, bytes bigint)
returns void language plpgsql security definer set search_path = '' as $$
begin
  if not public.is_admin() then raise exception 'no permitido' using errcode = '42501'; end if;
  if bytes is null or bytes < 0 or bytes > 1073741824 then raise exception 'cuota inválida' using errcode = '22023'; end if;
  update public.groups set image_quota_bytes = bytes where id = gid;
end $$;

-- Avisa que la persona está en un proyecto, con su nombre, la pantalla y un detalle (por ejemplo «escena 3»). Borra de paso lo
-- que lleva más de un día sin renovarse.
create or replace function public.set_presence(pid uuid, p_who text, p_screen text, p_detail text)
returns void language plpgsql security definer set search_path = '' as $$
begin
  if not public.is_member(public.project_group(pid)) then raise exception 'no permitido' using errcode = '42501'; end if;
  delete from public.presence p where p.project_id = pid and p.updated_at < now() - interval '1 day';
  insert into public.presence (project_id, user_id, who, screen, detail, updated_at)
  values (pid, auth.uid(), public.my_name(), left(coalesce(p_screen, ''), 40), left(coalesce(p_detail, ''), 120), now())
  on conflict (project_id, user_id) do update
    set who = excluded.who, screen = excluded.screen, detail = excluded.detail, updated_at = excluded.updated_at;
end $$;

-- Quién más está en el proyecto (renovó hace menos de 30 segundos), sin contar a quien pregunta.
create or replace function public.project_presence(pid uuid)
returns table (who text, screen text, detail text)
language plpgsql stable security definer set search_path = '' as $$
begin
  if not public.is_member(public.project_group(pid)) then raise exception 'no permitido' using errcode = '42501'; end if;
  return query
    select p.who, p.screen, p.detail from public.presence p
    where p.project_id = pid and p.user_id <> auth.uid() and p.updated_at > now() - interval '30 seconds'
    order by p.who;
end $$;

-- Un número que cambia cada vez que alguien sube algo al proyecto (la suma de las versiones de sus secciones): sirve para preguntar
-- «¿hubo cambios?» con una respuesta de unos pocos bytes en lugar de traer la lista de secciones.
create or replace function public.project_stamp(pid uuid)
returns bigint language plpgsql stable security definer set search_path = '' as $$
begin
  if not public.is_member(public.project_group(pid)) then raise exception 'no permitido' using errcode = '42501'; end if;
  return (select coalesce(sum(s.version), 0)::bigint from public.sections s where s.project_id = pid);
end $$;

-- Cada función solo la puede llamar quien inició sesión (nunca `anon`). Postgres da EXECUTE a todo el mundo (PUBLIC) por defecto:
-- hay que quitarlo de cada función ya creada, y recién después darlo a `authenticated`.
revoke all on all functions in schema public from public, anon, authenticated;
alter default privileges revoke execute on functions from public;
grant execute on function public.is_member(uuid), public.is_owner(uuid), public.is_admin(),
  public.project_group(uuid), public.shares_group_with(uuid),
  public.create_group(text), public.create_invite(uuid), public.join_group(text), public.leave_group(uuid),
  public.create_project(uuid, text), public.push_section(uuid, text, bigint, text, text),
  public.reserve_image(uuid, text, text, bigint), public.release_image(uuid, text),
  public.my_name(), public.set_display_name(text),
  public.group_usage(uuid), public.admin_usage(), public.admin_set_quota(uuid, bigint),
  public.set_presence(uuid, text, text, text), public.project_presence(uuid), public.project_stamp(uuid)
  to authenticated;

-- El perfil se crea solo al registrarse.
create or replace function public.handle_new_user()
returns trigger language plpgsql security definer set search_path = '' as $$
begin
  insert into public.profiles (id) values (new.id) on conflict do nothing;
  return new;
end $$;
drop trigger if exists on_auth_user_created on auth.users;
create trigger on_auth_user_created after insert on auth.users
  for each row execute function public.handle_new_user();

-- ------------------------------------------------------------------------------------------------
-- Storage: bucket privado de imágenes, con tamaño y tipos limitados también acá
-- ------------------------------------------------------------------------------------------------

insert into storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
values ('images', 'images', false, 2097152, array['image/jpeg', 'image/png', 'image/webp'])
on conflict (id) do update
  set public = false, file_size_limit = 2097152, allowed_mime_types = array['image/jpeg', 'image/png', 'image/webp'];

-- Los archivos se llaman «<grupo>/<proyecto>/<hash>.<ext>». Solo se puede subir uno que tenga reserva en `images` y que haya
-- reservado quien lo sube; leer, solo los miembros del grupo; borrar, solo los miembros del grupo.
drop policy if exists images_storage_read on storage.objects;
create policy images_storage_read on storage.objects for select to authenticated
  using (
    bucket_id = 'images'
    and (storage.foldername(name))[1] ~ '^[0-9a-f-]{36}$'
    and public.is_member(((storage.foldername(name))[1])::uuid)
  );

drop policy if exists images_storage_insert on storage.objects;
create policy images_storage_insert on storage.objects for insert to authenticated
  with check (
    bucket_id = 'images'
    and (storage.foldername(name))[1] ~ '^[0-9a-f-]{36}$'
    and (storage.foldername(name))[2] ~ '^[0-9a-f-]{36}$'
    and exists (
      select 1 from public.images i
      where i.group_id = ((storage.foldername(name))[1])::uuid
        and i.project_id = ((storage.foldername(name))[2])::uuid
        and i.uploaded_by = auth.uid()
        and name = i.group_id::text || '/' || i.project_id::text || '/' || i.hash || '.' || i.ext
    )
  );

drop policy if exists images_storage_delete on storage.objects;
create policy images_storage_delete on storage.objects for delete to authenticated
  using (
    bucket_id = 'images'
    and (storage.foldername(name))[1] ~ '^[0-9a-f-]{36}$'
    and public.is_member(((storage.foldername(name))[1])::uuid)
  );

-- ------------------------------------------------------------------------------------------------
-- Hacerse administrador (una sola vez, desde el SQL Editor, con TU usuario ya registrado):
--
--   insert into public.admins (user_id) select id from auth.users where email = 'tu-mail@ejemplo.com';
--
-- Como administrador se pueden ver el uso de todos los grupos y cambiar sus cuotas desde el programa (Nube → Administración)
-- o con:  select * from public.admin_usage();   select public.admin_set_quota('<id del grupo>', 104857600);
-- ------------------------------------------------------------------------------------------------
