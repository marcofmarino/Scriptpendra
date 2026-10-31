# Scriptpendra

Scriptpendra es una aplicación open-source pensada para llevar a cabo proyectos de cortometrajes universitarios contemplando las fases de escritura del guion, preproducción y producción.

## Trabajar en equipo

Un proyecto es una carpeta. Si la ponen en Drive, Dropbox o Syncthing, cada persona abre la suya y el programa se da cuenta cuando otra guarda algo (hay que tener la aplicación de escritorio y la carpeta vinculada).
Hay una opción de sincronizar por internet con un proyecto de [Supabase](https://supabase.com) propio: [`supabase/LEEME.md`](supabase/LEEME.md).

## Compilación

Se necesita [Rust](https://www.rust-lang.org/tools/install)
1.85 o una versión más nueva.

```sh
git clone https://github.com/marcofmarino/Scriptpendra
cd Scriptpendra
cargo run --release
```

## Licencia

GPL-3.0, el texto completo está en [`LICENSE`](LICENSE). Las fuentes de `assets/fonts` tienen sus propias licencias.
