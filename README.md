# GoFile DL en Rust

Abre `abrir.cmd` o `gofile-dl.exe`. Acepta varios enlaces, contraseña opcional y botones separados para listar, descargar, actualizar y cancelar. No requiere Python ni Playwright; usa una sesión propia de Edge/Chrome para obtener los enlaces oficiales y HTTP directo para descargar.

Los archivos y el seguimiento se guardan en `downloads/`. Los manifiestos y constancias anteriores se conservan e importan a SQLite, verificando tamaño y MD5 cuando existe. Los parciales permanecen ocultos junto a los metadatos. Las cookies y contraseñas no se guardan en la base ni en los registros.

El repositorio anterior `../gofile-dl` se conserva como respaldo. Se trasladaron sus 4 archivos y todo el seguimiento a `downloads/`, comprobando SHA-256 antes y después e importando los registros como completos. Las constancias y manifiestos anteriores tienen respaldo en `../gofile-dl/.migration-backup/`. El lanzador anterior también abre esta versión Rust.

## Compilar

Este repositorio es independiente: incluye el motor, los módulos de los sitios y las pruebas. No requiere clonar otros repositorios. Cada repositorio mantiene su propia copia del motor; las correcciones comunes deben aplicarse a cada copia.

```powershell
cargo build --release --locked
# Ejecutar la aplicación recién compilada:
.\target\release\gofile-dl.exe
```

## Desarrollo y estructura

En Windows instala Rust estable con el toolchain MSVC y las herramientas de compilación de Visual Studio (C++ y Windows SDK). Cargo descarga las dependencias públicas indicadas en `Cargo.lock`.

- `src/main.rs`: entrada de la aplicación gráfica.
- `src/lib.rs`: tipos comunes y módulos de la biblioteca local.
- `src/gui.rs`: interfaz; `src/engine.rs`: cola y transferencias.
- `src/transport.rs`: HTTP; `src/state.rs`: SQLite e integridad.
- `src/sites/`: adaptadores de los sitios incluidos en la copia del motor.
- `tests/`: pruebas con datos sintéticos y servidores locales.

```powershell
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
powershell -NoProfile -File tools/verify-standalone.ps1
```

El único binario de este paquete es la aplicación gráfica. `abrir.cmd` abre el ejecutable situado en la raíz. Para actualizarlo, cierra la aplicación y copia el archivo compilado desde `target/release/` a la raíz. Las fuentes no incluyen los ejecutables ni `downloads/`, que contiene los medios y su seguimiento privado.

## Licencia y uso responsable

El código propio se distribuye bajo la [licencia MIT](LICENSE). El propósito educativo y las responsabilidades de uso se describen en [USAGE_NOTICE.md](USAGE_NOTICE.md). Esta licencia no concede derechos sobre contenido ajeno.
