TURNER — REVISIÓN 4 PARA MAC INTEL
Adaptación de themoretheless/tuner v0.1.13
Base: 95dd6125053f86e361cdc51b594c7d44ca5c5867
Fuentes: https://github.com/Chevosky/tuner-compact (rama compact-intel)
Actualizaciones: https://github.com/Tangram-Space/updates
Fecha: 21 de octubre de 2026

CAMBIOS DE ESTA REVISIÓN

El nombre visible de la app es Turner. El botón del micrófono muestra un
jack de instrumento de 6,35 mm mono TS: punta, banda aislante, cuerpo y cable.
Conserva su área de 30 x 28 px y las acciones existentes. Verde mientras
captura, rojo al silenciar y ámbar durante el inicio, que puede cancelarse.
El icono de la aplicación —diapasón negro sobre un círculo verde— conserva
exactamente los mismos archivos SVG, PNG, ICO e ICNS de la revisión 3.

La ventana, los paneles y sus funciones se mantienen. Se conserva el bundle
ID com.tuner.guitar.compact, el ejecutable guitar-tuner-egui y la identidad
de preferencias Guitar Tuner. El nuevo nombre no reinicia los ajustes.

El actualizador consulta el canal estable propio de Turner en el repositorio
compartido Tangram-Space/updates. No usa la release global más reciente.
Comprueba app_id, canal, tag exacto, plataforma, identidad, manifiesto,
checksum y paquete antes de instalar. El feed y el manifiesto de la release
deben coincidir por completo. Una release indicada pero incompleta produce
un error visible y no se confunde con estar actualizado.

INSTALAR O MIGRAR DESDE R3

Release canónica:
https://github.com/Tangram-Space/updates/releases/tag/turner-v0.1.13-r4
Paquete: Turner-v0.1.13-r4-macos-x86_64.app.zip

Instalación manual:
1. Cerrar la app anterior y descomprimir el ZIP.
2. Mover Turner.app a Aplicaciones. Si existe Tuner Compact.app, retirar la
   copia anterior al instalar manualmente, para conservar una sola app.
3. Si macOS solicita confirmar la apertura por desarrollador no identificado,
   usar clic derecho sobre la app y elegir Abrir.
4. En Ajustes, elegir la entrada. Pulsar el jack para iniciar y permitir el
   acceso al micrófono cuando macOS lo solicite.
5. Comprobar iniciar, silenciar y volver a iniciar con la entrada del iMac.

La r3 instalada todavía consulta el repositorio anterior. El puente r4 se
publica allí sólo después de verificar la release central. Contiene el mismo
ejecutable nuevo y usa el tag, nombres de archivos y raíz del ZIP que r3
entiende. Buscar actualizaciones permite instalar ese puente en la misma
ruta y, desde entonces, consultar el canal central. No crea una segunda app.
El archivo puede conservar el nombre Tuner Compact.app; con la app cerrada
se puede renombrar a Turner.app. La identidad del bundle y los ajustes se
conservan. La instalación canónica nueva ya usa Turner.app.

FUNCIONES CONSERVADAS

Panel principal:
  - Arco de -50 a +50 cents, nota digital, octava, alteración y flechas.
  - Frecuencia, cents reales, nivel de entrada y referencia A4.
  - Jack de micrófono, afinación, Ajustes, Análisis y detener tono activo.
  - Los errores reales permanecen visibles; los avisos están en Análisis.

Selector de afinación:
  - Botón compacto con el nombre actual y panel con búsqueda por instrumento.
  - Los mismos 33 presets e índices guardados; búsqueda sin mayúsculas/tildes.
  - Enter selecciona un único resultado; Escape o clic fuera cierra el panel.

Ajustes:
  - Los 33 presets, incluidos cromático y afinaciones por cuerda.
  - Edición de cuerdas entre 20 y 1200 Hz y A4 entre 420 y 460 Hz.
  - Entrada predeterminada o seleccionada, refrescar lista y reiniciar captura.
  - Tono de referencia y nota aleatoria para entrenamiento auditivo.
  - Versión instalada, Buscar actualizaciones y Actualizar y reiniciar.

Análisis:
  - Nota detectada y objetivo del indicador, frecuencia, cents y confianza.
  - Nivel de entrada, power chord, forma de onda e historial de cents.
  - Espectro, espectrograma y diagnósticos de señal y recuperación.

Atajos: Espacio o M = micrófono; R = alternar/detener tono de referencia.
No se activan mientras se escribe en un campo. Se conservan las preferencias
de A4, preset, dispositivo, espectro y espectrograma. La edición de frecuencias
de cuerdas no persiste entre sesiones, igual que en el original.

El motor de detección no cambia. En presets de cuerda, la nota central y los
cents describen la misma referencia; la nota cromática detectada también
permanece disponible. Sin frecuencia válida no se muestra afinado.

AUDIO: CORRECCIONES HEREDADAS DE R3

Enumeración, apertura, pausa y destrucción de audio se ejecutan en un worker,
fuera del hilo de la interfaz. La ventana no espera la respuesta del permiso
ni el cierre del dispositivo. La cancelación invalida los callbacks y evita
que una apertura tardía reactive una sesión detenida. Los cambios rápidos
de entrada se serializan. El procesador termina por cancelación aunque el
backend conserve su sender y la captura pausa explícitamente el stream antes
de destruirlo.

El backport acotado del fix oficial CPAL #869 usa Weak en el listener CoreAudio
para eliminar un ciclo que retenía streams de dispositivos elegidos por nombre.
Se conserva CPAL 0.15.3 bajo vendor/cpal; ver TUNER-PATCH.md y Apache-2.0.

Detener, reemplazar o cerrar un tono invalida su callback y lo silencia en
el siguiente buffer, aunque el worker esté esperando al sistema operativo.
Cancelar no aborta una llamada nativa bloqueada: su limpieza termina cuando
esa llamada retorna. No se crea otro intento nativo simultáneo.

ACTUALIZACIONES DESDE LA APP

1. En Ajustes, pulsar Buscar actualizaciones junto a la versión.
2. Si hay una revisión posterior, pulsar Actualizar y reiniciar.
3. Puede cancelarse mientras consulta, descarga o prepara el paquete. La
   ventana se cierra sólo después de confirmar que el instalador está listo.
4. El instalador espera que salga el proceso, guarda la app anterior y mueve
   la nueva a la misma ruta. Solicita abrirla de nuevo con macOS. El cierre
   normal de eframe conserva las preferencias existentes.

Feed:
https://raw.githubusercontent.com/Tangram-Space/updates/main/feeds/turner/stable/release.json

El backup queda en un directorio oculto .turner-update-… junto a la app, con
Previous.app y un registro result.txt/install.log. El instalador de la r3
puede usar el prefijo anterior .tuner-compact-update-… durante la migración.
Si falla el reemplazo o macOS devuelve un error al solicitar la apertura,
se intenta restaurar la copia anterior. Si también falla la restauración,
el backup se conserva y se registra su ruta. La solicitud de apertura exitosa
no demuestra que la app no se cierre después: no hay chequeo de salud.

Requiere una carpeta de instalación escribible por el usuario. No solicita
privilegios de administrador ni instala un servicio. No reemplaza una app
translocada ni una app en un volumen de sólo lectura. El afinador funciona
sin Internet. La consulta es manual y no requiere cuenta o token GitHub.
HTTPS y SHA-256 comprueban transporte e integridad contra el repositorio;
no constituyen firma independiente del editor ni notarización de Apple.

COMPILAR Y PUBLICAR

Mac Intel, macOS 11 o posterior, Xcode o Command Line Tools con SDK macOS y
Rust 1.96.0 mediante rustup. No requiere Homebrew. Desde el proyecto:

  bash scripts/build-compact-macos.command

El script no instala herramientas. Usa Cargo.lock, target Intel y
MACOSX_DEPLOYMENT_TARGET=11.0; comprueba arquitectura, mínimo del ejecutable,
plist, permiso de micrófono, icono y ZIP. Produce Turner.app y el paquete
nuevo en compact-builds/, sin reemplazar aplicaciones instaladas.
CFBundleVersion procede de COMPACT_REVISION en egui/src/version.rs; la
revisión sigue aumentando aunque la versión base permanezca en 0.1.13.

El workflow .github/workflows/build-compact-macos.yml de este repositorio
sólo compila manualmente. La publicación canónica se solicita cambiando
apps/turner/release.json en Tangram-Space/updates: debe fijar repositorio,
commit de 40 caracteres, versión y revisión. El workflow central usa
macos-15-intel; valida la solicitud contra las fuentes, ejecuta pruebas,
compila y verifica el ZIP real. La plantilla revisable se conserva en
ci/turner/turner-build.yml de este repositorio de fuentes.

El build usa permisos de sólo lectura. Un job separado publica el ZIP,
SHA-256 y turner-update.json en turner-vVERSION-rREVISION. Primero completa
la release, después actualiza el feed de Turner con el manifiesto exacto.
No sobrescribe releases ni canales de otras apps y no modifica latest global.
Un cambio concurrente del repositorio o del feed impide avanzar el puntero
sin revisar el nuevo estado. El puente de r3 se solicita explícitamente sólo
después de verificar la publicación central; no recompila el ejecutable.

VALIDACIÓN Y ALCANCE

La suite comprueba permisos lentos, cancelación, cierre con sender retenido,
solicitudes obsoletas y silencio inmediato del tono. Comprueba selección de
presets, selector y estados de actualización. El actualizador cubre selección
por aplicación/canal, coincidencia del feed con la release, URLs, integridad,
ZIP inseguros, Mach-O, ruta instalada legacy, cancelación y confirmación del
helper. El instalador Bash se prueba con utilidades del sistema simuladas:
apertura aceptada/fallida, respaldo, restauración y rutas especiales.

El workflow nativo prueba el parser con el ZIP y ejecutable recién compilados.
Las pruebas de publicación usan una API simulada para evitar modificar releases
durante los tests. Los informes de Actions identifican el commit probado.

Referencia histórica r3: 51 pruebas Rust integradas en Linux y macOS Intel,
y 26 pruebas del publicador; compilación y publicación correctas:
https://github.com/Chevosky/tuner-compact/actions/runs/39624312120

Estas comprobaciones no sustituyen probar permisos, inicio, pausa y reinicio
con la BRIO en Big Sur 11.7.10. No se dispone de una traza del proceso que se
colgó en el iMac. El reemplazo y reapertura completos de una app en ejecución
mediante este actualizador aún requieren una prueba real en macOS.

ORIGEN Y LICENCIAS

Proyecto base: https://github.com/themoretheless/tuner/tree/v0.1.13
Se conserva MIT del proyecto y Apache-2.0 de CPAL, incluidos en
Contents/Resources/Licenses del bundle. Backport:
https://github.com/RustAudio/cpal/pull/869
