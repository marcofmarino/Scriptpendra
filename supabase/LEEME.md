# Scriptpendra con Supabase

## Puesta en marcha (una sola vez, la hace quien administra)

1. Crear un proyecto en <https://supabase.com> (plan gratuito).
2. **SQL Editor** → pegar `schema.sql` entero → *Run*. Se puede volver a correr sin borrar datos.
3. **SQL Editor** → pegar `tests.sql` → *Run*. Tiene que terminar con `TODAS LAS PRUEBAS PASARON`. Si no, no usar la base.
4. **Authentication → Providers → Email**: dejar activado **Confirm email**. Eso se configura en el panel de Supabase (no se puede
   dejar hecho desde el código ni desde `schema.sql`). Con eso activado, quien crea una cuenta recibe un mail y no puede iniciar
   sesión hasta abrirlo; el programa le avisa «Falta confirmar el mail». Mientras tanto, en el mismo lugar conviene poner la
   longitud mínima de contraseña en 10 (el programa ya la pide al crear la cuenta).
5. **Authentication → Email Templates → Reset Password**: reemplazar el enlace del mensaje por el código, por ejemplo:
   `<p>Tu código para recuperar la contraseña de Scriptpendra es:</p><h2>{{ .Token }}</h2>`
   El programa no tiene página web a la que mande el enlace: la persona escribe el código en «Olvidé mi contraseña». El
   código vence a la hora. (Con el servicio de mails por defecto del plan gratuito se mandan muy pocos por hora; si hace falta,
   configurar un SMTP propio en **Project Settings → Auth → SMTP**.)
6. Registrarte desde el programa y después, en el SQL Editor:
   `insert into public.admins (user_id) select id from auth.users where email = 'tu-mail@ejemplo.com';`
7. **Project Settings → API Keys**: copiar la **Project URL** (algo como `https://xxxx.supabase.co`) y la clave pública: la
   **Publishable key** (empieza con `sb_publishable_`) o, en la pestaña de claves anteriores (*Legacy*), la **anon public**.
   El programa rechaza las claves secretas (`sb_secret_…` o `service_role`) para que no se peguen por error. Esas dos se las pasás a los grupos (se pegan en el
   programa: ☰ → Nube…). **La clave `service_role` es secreta: no se comparte ni se pega en el programa.**

## Administrar el espacio de imágenes

* Cada grupo tiene una cuota (50 MB de fábrica). Cada imagen: 2 MB como máximo, solo JPG, PNG o WebP; hasta 60 por hora por
  persona. Todo eso lo controla el servidor, no solo el programa.
* Ver el uso de todos los grupos: `select * from public.admin_usage();`
* Cambiar la cuota de un grupo: `select public.admin_set_quota('<id del grupo>', 104857600);` (100 MB).
* Quienes usan el programa ven su uso en ☰ → Nube… y pueden quitar las imágenes que ya no se usan.
* El plan gratuito de Supabase da 1 GB de archivos en total: la suma de las cuotas no debería pasarse de eso.

## Límites del plan gratuito a tener en cuenta

* Se pausa tras una semana sin uso (hay que reactivarlo desde el panel).
* No tiene copias de seguridad automáticas. Las copias de trabajo viven en las carpetas de cada persona y en
  «Versiones anteriores».

## Reservas de imágenes sin archivo

Si una computadora se apaga justo entre reservar el lugar de una imagen y subirla, la reserva queda en la base sin archivo. Se
completa sola cuando esa persona (u otra con la misma imagen) vuelve a sincronizar. Si nadie vuelve, ocupa cuota. Para verlas y
limpiarlas (por ejemplo una vez por semana):

```sql
-- ver las reservas viejas sin archivo
select i.group_id, i.project_id, i.hash, i.size, i.created_at
from public.images i
left join storage.objects o
  on o.bucket_id = 'images'
 and o.name = i.group_id || '/' || i.project_id || '/' || i.hash || '.' || i.ext
where o.id is null and i.created_at < now() - interval '2 days';

-- liberarlas
delete from public.images i
where i.created_at < now() - interval '2 days'
  and not exists (
    select 1 from storage.objects o
    where o.bucket_id = 'images'
      and o.name = i.group_id || '/' || i.project_id || '/' || i.hash || '.' || i.ext);
```

## Compilar con la nube incorporada (opcional)

El código del repositorio no lleva ninguna dirección ni clave de la nube: cada persona las pega en ☰ → Nube…. Si querés repartir un
ejecutable que ya venga configurado, la URL y la clave pública se incorporan al compilar:

```sh
SCRIPTPENDRA_CLOUD_URL="https://xxxx.supabase.co" \
SCRIPTPENDRA_CLOUD_KEY="<clave anon>" \
cargo build --release
```

En GitHub Actions, la URL y la clave se guardan como *variables* del repositorio (Settings → Secrets and variables → Actions →
Variables) y se pasan como variables de entorno al paso de compilar. **Nunca** se escriben en un archivo del repositorio.

La clave `anon` y la URL no son secretas (cualquiera que tenga el ejecutable las puede sacar): lo que protege los datos es la
seguridad por fila de `schema.sql`. Por eso es importante correr `tests.sql`. La `service_role`, en cambio, no va nunca ni en el
código ni en el ejecutable.

### Qué tener en cuenta con el registro abierto

Como cualquiera con la URL y la clave puede crear una cuenta, conviene en **Authentication**:
* dejar activada la confirmación por mail;
* activar CAPTCHA (hCaptcha o Turnstile) si ven registros que no corresponden;
* revisar de vez en cuando `select count(*) from auth.users;` y `select * from public.admin_usage();`.

El esquema ya limita: 5 grupos por persona, 300 grupos en total, 30 proyectos por grupo y la cuota de imágenes por grupo.
