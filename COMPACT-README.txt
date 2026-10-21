TUNER COMPACT — INTERFAZ NATIVA PARA MAC INTEL
Adaptación de themoretheless/tuner v0.1.13
Base: 95dd6125053f86e361cdc51b594c7d44ca5c5867
Fork público: https://github.com/Chevosky/tuner-compact
Rama de esta adaptación: compact-intel
Fecha: 21 de octubre de 2026

ESTADO DE ESTA ENTREGA

La compilación de la rama compact-intel terminó correctamente en macOS Intel
el 21 de octubre de 2026. Se generó Tuner Compact.app dentro de un ZIP y se
verificaron su arquitectura x86_64, el macOS mínimo declarado 11.0, el plist,
la descripción de uso del micrófono, el icono y la integridad del ZIP.

Ejecución verificada:
https://github.com/Chevosky/tuner-compact/actions/runs/39606802118
Código compilado: 56af317a8402c7b8d7481be59243521673cdf593

Las 14 pruebas del cliente nativo pasaron tanto en Linux como en el runner
macOS Intel, incluidas dos regresiones nuevas sobre coherencia de nota/cents
y silencio.
La interfaz real también se ejecutó en Linux para comprobar los tamaños
680 x 340 y 520 x 290, los presets con nombres largos, el cierre y reapertura
de las ventanas de Ajustes y Análisis, y el acceso a sus controles.
La validación final de captura de micrófono y ejecución en Big Sur 11.7.10
sigue pendiente y debe hacerse con el binario macOS en el iMac.

INSTALAR LA APP EN EL IMAC

1. Abrir la ejecución de GitHub Actions enlazada arriba y descargar el
   artefacto "Tuner-Compact-macos-Intel".
2. Descomprimir el ZIP descargado. Si dentro aparece
   Tuner-Compact-v0.1.13-macos-x86_64.app.zip, descomprimirlo también hasta
   obtener Tuner Compact.app.
3. Mover Tuner Compact.app a la carpeta Aplicaciones y abrirla.
4. Si macOS la bloquea por desarrollador no identificado, hacer clic derecho
   sobre Tuner Compact.app y elegir Abrir; confirmar la apertura.
5. En Ajustes, elegir la entrada de audio. Pulsar Escuchar y permitir el
   acceso al micrófono cuando macOS lo solicite.

El artefacto de Actions tiene una retención de 14 días, hasta el 4 de
noviembre de 2026. La descarga corresponde a esa ejecución; no se creó
una publicación en Releases.

DISEÑO Y CONTROLES

Panel principal:
  - Arco graduado de -50 a +50 cents.
  - Nota digital con octava y alteración.
  - Flechas para subir o bajar el tono; verde cuando el motor indica afinado.
  - Frecuencia, cents reales, nivel de entrada y referencia A4.
  - Iniciar/detener micrófono, selector de afinación, Ajustes y Análisis.
  - Botón para detener un tono activo, errores y acceso a los avisos.

Ajustes (ventana independiente con desplazamiento vertical):
  - Los mismos 33 presets, con el mismo orden e índices guardados.
  - El preset cromático existente aparece como "Cromático · todos los
    instrumentos"; mantiene el comportamiento del original.
  - Edición de cada cuerda entre 20 y 1200 Hz.
  - Referencia A4 entre 420 y 460 Hz.
  - Entrada de audio, dispositivo predeterminado y actualización de la lista.
  - Iniciar/detener micrófono y reinicio de captura al cambiar entrada.
  - Tono de referencia y nota aleatoria para entrenamiento auditivo.

Análisis (ventana independiente con desplazamiento vertical):
  - Nota detectada, referencia del indicador, frecuencia, cents y confianza.
  - Nivel de entrada y detección de power chord.
  - Forma de onda, medidor lineal e historial de cents.
  - Espectro y espectrograma, con controles independientes.
  - Diagnósticos de señal, errores de audio y recuperación del dispositivo.

Atajos: Espacio o M = micrófono; R = alternar/detener tono de referencia.
No se activan mientras se está escribiendo en un campo.

Se conservan las preferencias de A4, preset, entrada, espectro y espectrograma.
El tamaño inicial cambia una sola vez al formato compacto; luego se respeta
el tamaño elegido por el usuario. Las frecuencias de cuerdas editadas no se
guardan entre sesiones, igual que en la versión original.

COHERENCIA DE LA LECTURA

En un preset de cuerdas, los cents se calculan contra una cuerda objetivo.
La nota central ahora muestra esa misma referencia. La nota cromática
detectada permanece disponible en el panel y en Análisis. En modo cromático,
la nota central es la nota detectada. Sin frecuencia válida no se ilumina
el estado de afinado. El arco llega hasta +/-50, pero el texto de cents
conserva el valor real aunque esté fuera de ese rango.

El procesamiento de audio y los algoritmos de detección no se modificaron.
La interfaz usa geometría vectorial nativa de egui; no requiere imágenes,
fuentes nuevas, navegador ni servicios externos para funcionar.

COMPILAR EN EL IMAC

Requisitos: Mac Intel con macOS 11, Xcode o Command Line Tools con SDK macOS,
y Rust 1.96.0 instalado mediante rustup. No hace falta Homebrew.

1. Descargar el código de la rama compact-intel y descomprimir la carpeta
   completa del proyecto.
2. Abrir Terminal, ubicarse en la raíz del proyecto (la carpeta que contiene
   Cargo.toml) y ejecutar:

     bash scripts/build-compact-macos.command

   No hace falta abrir el .command con doble clic ni darle permiso de
   ejecución. Si falta una herramienta, el script se detiene y muestra las
   instrucciones correspondientes; no la instala por su cuenta. Cargo puede
   descargar dependencias durante el build.

3. Al terminar, buscar la nueva carpeta creada dentro de compact-builds/.
   Allí se encontrarán Tuner Compact.app y su ZIP.
4. Abrir Tuner Compact.app, permitir el micrófono y elegir la entrada de audio.

El script usa Cargo.lock, Rust 1.96.0, el target x86_64-apple-darwin y el mínimo
MACOSX_DEPLOYMENT_TARGET=11.0. Verifica la arquitectura, el mínimo de macOS,
el plist, la descripción de uso del micrófono, el icono y el ZIP. Esa verificación del
paquete no reemplaza la prueba de funcionamiento en el iMac.

COMPILAR EN GITHUB ACTIONS

Fork: https://github.com/Chevosky/tuner-compact
Rama a compilar: compact-intel
Definición: .github/workflows/build-compact-macos.yml

El workflow "Build Tuner Compact for Intel" se inicia manualmente desde
Actions sobre la rama compact-intel. Usa un runner macos-15-intel, ejecuta
las pruebas del cliente en macOS y produce el ZIP mediante
scripts/build-compact-macos.command.

La ejecución 39606802118 terminó con éxito: 14 pruebas aprobadas, compilación
release y verificaciones del paquete correctas. El artefacto generado en
Actions es "Tuner-Compact-macos-Intel" (ID 12349431371), con retención de
14 días. El workflow compacto no crea releases ni tags; los permisos del
repositorio son sólo de lectura durante el build.

Compilar y revisar el paquete en macOS 15 no reemplaza la prueba de audio y
funcionamiento en el iMac con Big Sur 11.7.10.

ORIGEN Y LICENCIA

Proyecto base: https://github.com/themoretheless/tuner/tree/v0.1.13
Se conserva la licencia MIT y los avisos del proyecto original en LICENSE.
El icono es el que ya trae Tuner. El panel adapta la disposición de la
referencia visual aportada por el usuario con elementos vectoriales propios.

Documentación de los requisitos de build:
https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html
https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job
