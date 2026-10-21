TUNER COMPACT — REVISIÓN 3 PARA MAC INTEL
Adaptación de themoretheless/tuner v0.1.13
Base: 95dd6125053f86e361cdc51b594c7d44ca5c5867
Fork: https://github.com/Chevosky/tuner-compact
Rama: compact-intel
Fecha: 21 de octubre de 2026

CAMBIOS DE ESTA REVISIÓN

Esta revisión incorpora Buscar actualizaciones junto a la versión en Ajustes.
La consulta es manual; el afinador sigue funcionando sin conexión. Cuando
GitHub publica una revisión posterior, Actualizar y reiniciar descarga el
paquete y verifica su integridad, identidad y compatibilidad antes del
reemplazo. Se conserva una copia anterior. No requiere login de GitHub.
La primera instalación con esta función debe hacerse con el ZIP de la app.
Los cambios de interfaz y audio descritos a continuación también se incluyen.

El control del micrófono es un plug de 30 x 28 px: verde mientras captura,
rojo al silenciar. Durante el inicio aparece ámbar y se puede cancelar.
El nombre del dispositivo y la acción aparecen al pasar el mouse; el selector
de entrada sigue disponible en Ajustes. Se quitaron las filas redundantes
de LED y dispositivo. El tamaño inicial pasa una sola vez a 680 x 294;
después se conserva el tamaño elegido por el usuario (mínimo 520 x 290).

El selector de afinación es un botón discreto con el nombre actual. Abre un
panel con búsqueda y grupos de instrumentos; conserva los 33 presets y sus
índices guardados. La búsqueda ignora mayúsculas y tildes. Enter selecciona
cuando hay un solo resultado; Escape o un clic fuera cierran el selector.

El icono de Finder y el de la app abierta usan el mismo diapasón negro sobre
un círculo verde, con transparencia fuera del círculo. El PNG de ejecución
y el ICNS del paquete proceden del mismo SVG en assets/icons/.

CORRECCIÓN DEL INICIO Y LA PAUSA

La enumeración, apertura, pausa y destrucción de audio se ejecutan en un
worker, fuera del hilo de la interfaz. La ventana no espera la respuesta de
un permiso ni el cierre del dispositivo. Se quitó el timeout fijo de dos
segundos que conducía a esperar un join al iniciar. La cancelación invalida
los callbacks y evita que una apertura tardía reactive una sesión detenida.
Los cambios rápidos de entrada se serializan.

El procesador termina por cancelación aunque el backend conserve su sender.
La captura pausa explícitamente el stream antes de destruirlo. Además se
incluye un backport acotado del fix oficial CPAL #869: el listener CoreAudio
usa Weak para eliminar un ciclo que retenía streams de dispositivos elegidos
por nombre. Se conserva CPAL 0.15.3 y sus demás dependencias, bajo vendor/cpal;
ver vendor/cpal/TUNER-PATCH.md y su licencia Apache-2.0.

Detener, reemplazar o cerrar un tono invalida su callback: se silencia en el
siguiente buffer aunque el worker esté esperando al sistema operativo.
Cancelar no aborta una llamada nativa que ya esté bloqueada: su limpieza
termina cuando esa llamada retorna. No se crea otro intento nativo simultáneo.

VALIDACIÓN

En Linux pasaron 51 pruebas integradas: 16 de audio-input y 35 del cliente,
incluidas las regresiones de permisos lentos, cancelación, cierre con sender
retenido, pool lleno, solicitudes obsoletas y silencio inmediato del tono.
La selección de los 33 presets se comprobó con eventos de egui, incluidos
búsqueda, Enter, Escape y límites de tamaño a 520 x 290 y 680 x 294.
También pasaron 26 pruebas del proceso de publicación con API GitHub simulada.
Las pruebas del actualizador cubren manifest y URLs, integridad, rechazo de
ZIP inseguros, Mach-O, cancelación, confirmación del helper y el instalador
Bash con utilidades del sistema simuladas. Se probó apertura aceptada y
fallida, conservación/restauración del respaldo y rutas con espacios,
comillas y caracteres especiales. Los parsers aceptaron el ZIP y el ejecutable
Mac Intel reales de la primera revisión. El reemplazo real en macOS aún no
se ha ejecutado en este entorno.
El workflow indicado abajo compila en macOS y verifica el paquete antes de
publicarlo. Una release sólo está disponible si esas comprobaciones pasan.
Estas comprobaciones no sustituyen la prueba física con Big Sur 11.7.10 y BRIO.
No se dispone de una traza del proceso que se colgó en el iMac.

INSTALAR EN EL IMAC CUANDO ESTÉ COMPILADA

Este paquete de fuentes no incluye una app r3 compilada. Los siguientes
pasos corresponden al ZIP que genere el script de compilación o una futura
ejecución exitosa del workflow con el código completo de esta revisión.

1. Cerrar la app anterior antes de reemplazarla.
2. Descomprimir Tuner-Compact-v0.1.13-r3-macos-x86_64.app.zip.
   Si la descarga de Actions contiene ese ZIP dentro, extraer ambos niveles.
3. Mover Tuner Compact.app a Aplicaciones y abrirla desde allí.
4. Si macOS solicita confirmar la apertura por desarrollador no identificado,
   usar clic derecho sobre la app y elegir Abrir.
5. En Ajustes, elegir la entrada de audio. Pulsar el plug para iniciar y
   permitir el acceso al micrófono cuando macOS lo solicite.
6. Comprobar iniciar, silenciar y volver a iniciar. El plug debe responder
   sin que se inmovilice la ventana. Si el Dock conserva un icono antiguo,
   quitar ese acceso del Dock y volver a agregar la app instalada.

FUNCIONES CONSERVADAS

Panel principal:
  - Arco de -50 a +50 cents, nota digital, octava, alteración y flechas.
  - Frecuencia, cents reales, nivel de entrada y referencia A4.
  - Micrófono, afinación, Ajustes, Análisis y detener tono activo.
  - Los errores reales permanecen visibles; los avisos están en Análisis.

Ajustes:
  - Los mismos 33 presets, incluidos el modo cromático y afinaciones por cuerda.
  - Edición de cada cuerda entre 20 y 1200 Hz y A4 entre 420 y 460 Hz.
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

ACTUALIZACIONES DESDE LA APP

1. En Ajustes, pulsar Buscar actualizaciones junto a la versión.
2. Si hay una revisión posterior, pulsar Actualizar y reiniciar.
3. Puede cancelarse mientras consulta, descarga o prepara el paquete. La
   ventana se cierra sólo después de confirmar que el instalador está listo.
4. El instalador espera que salga el proceso, guarda la app anterior y mueve
   la nueva a la misma ruta. Solicita abrirla de nuevo con macOS. El cierre
   normal de eframe conserva A4, preset y las demás preferencias existentes.

El backup queda en un directorio oculto .tuner-compact-update-… junto a la app,
con Previous.app y un registro result.txt/install.log. No aparece otra app
visible en Aplicaciones. Si falla el reemplazo o macOS devuelve un error al
solicitar la apertura, se intenta restaurar la copia anterior. Si también
falla esa restauración por permisos o error de disco, el backup se conserva
y se registra su ruta. La solicitud de apertura exitosa no demuestra que la
nueva app no se cierre después: no hay un chequeo automático de salud.

La actualización requiere una carpeta de instalación escribible por el
usuario. No solicita privilegios de administrador, no instala un servicio
y no puede reemplazar una app translocada o en un volumen de sólo lectura.
El afinador sigue funcionando sin Internet. La consulta es manual y no
necesita una cuenta o token GitHub. HTTPS y SHA-256 verifican origen de
transporte e integridad contra el repositorio configurado; no constituyen
una firma de editor independiente ni notarización de Apple.

COMPILAR

Mac Intel, macOS 11 o posterior, Xcode o Command Line Tools con SDK macOS y
Rust 1.96.0 instalado mediante rustup. No requiere Homebrew.
Desde la carpeta completa del proyecto:

  bash scripts/build-compact-macos.command

El script no instala herramientas. Usa Cargo.lock, el target Intel y
MACOSX_DEPLOYMENT_TARGET=11.0; verifica arquitectura, mínimo del ejecutable,
plist, permiso de micrófono, icono y ZIP. Produce un paquete nuevo dentro de
compact-builds/ sin reemplazar aplicaciones instaladas. La revisión del bundle
(CFBundleVersion) procede de egui/src/version.rs y el ZIP lleva el sufijo r3.

Workflow: .github/workflows/build-compact-macos.yml
Nombre: Build Tuner Compact for Intel
Inicio manual sobre compact-intel; runner macos-15-intel. Ejecuta las pruebas
de publicación, audio-input y del cliente, compila y sube
Tuner-Compact-macos-Intel. El artefacto se retiene 14 días.

El inicio manual sólo publica una Release si se activa publish. También
puede publicar al subir a compact-intel un incremento deliberado de
COMPACT_REVISION en egui/src/version.rs. La revisión es creciente aunque
la versión base permanezca 0.1.13. El build tiene permisos de sólo lectura;
otro job con permiso de contenido publica después de validar el resultado.
La publicación se permite únicamente desde la rama predeterminada vigente.

Cada release incluye el ZIP de la app, su archivo .sha256 y el manifiesto
tuner-compact-update.json. Se prepara como borrador, se cargan los tres
archivos y sólo se publica al comprobar que están completos. Los tags y
releases existentes no se sobrescriben. Los tests, la arquitectura Intel,
macOS mínimo 11.0, identidad de la app y checksum son requisitos previos.

ORIGEN Y LICENCIAS

Proyecto base: https://github.com/themoretheless/tuner/tree/v0.1.13
Se mantiene la licencia MIT del proyecto. CPAL conserva Apache-2.0.
Ambas licencias se incluyen en Contents/Resources/Licenses de la app.
Backport: https://github.com/RustAudio/cpal/pull/869
