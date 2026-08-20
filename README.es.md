# tabs-repo

Aplicación de escritorio para capturar, guardar, imprimir y reescribir tablaturas de guitarra.

---

## Por qué existe

Muchas de las canciones que quiero tocar no están en tablatura en ninguna parte de internet. Las
saco a mano mirando videos de YouTube: pausar, retroceder, bajar la velocidad, anotar, repetir.
Ese trabajo existe, cuesta horas, y hasta ahora se perdía — no estaba en un Drive, ni en un
repositorio, ni en ningún lado del que pudiera recuperarlo.

Hay editores de tablatura que resuelven una parte de esto. Podría haberlos usado. Pero prefiero
construir mi propia herramienta antes que acomodarme a la de otro: se aprende más, se entiende lo
que se usa, y la herramienta termina haciendo exactamente lo que necesito en vez de lo que alguien
supuso que yo necesitaba. Esa preferencia es deliberada y es el motivo de que este repositorio
exista.

Además hay tres cosas que no encontré resueltas en ninguna parte:

**Ajustar la dificultad de forma gradual.** Las tablaturas que circulan vienen en dos extremos: o
son versiones de principiante que aburren, o transcripciones nota por nota que no hay manera de
tocar. Falta el punto medio. Quiero pedirle a la aplicación *un poco* más difícil y que le meta
arreglos con criterio sin volver la pieza impracticable.

**Saber qué me sé de verdad.** No una lista de canciones marcadas, sino a qué velocidad me sale
hoy cada una frente a su velocidad real, y en qué compases me trabo.

**Capturar sin fricción.** El video y el editor en la misma pantalla, con bucle A-B, media
velocidad y retroceso de tres segundos a un atajo de distancia.

## Qué hace

- **Captura** desde YouTube: reproductor integrado con bucle A-B, velocidad reducida y retroceso
  rápido, junto a un editor con entrada por teclado y diapasón clicable.
- **Repositorio** de tablaturas propias, con búsqueda, etiquetas e historial de versiones.
- **Progreso**: estado por canción, velocidad actual frente a la objetivo, y marcado de los
  compases problemáticos.
- **Impresión**: hoja legible desde el atril, con notación estándar y diagramas de acordes
  opcionales.
- **Transformación de dificultad**: simplificar o añadir arreglos, con un validador que garantiza
  que lo generado se puede tocar de verdad.

## Estado

En construcción, pero **ya se puede usar para transcribir**: se abre un vídeo, se escribe con el
teclado y se guarda.

| Hito | Contenido | Estado |
|---|---|---|
| M0 | Verificación de Tauri + alphaTab + YouTube | ✅ completado |
| M1 | Modelo de datos y serialización | ✅ completado |
| M2 | Captura rápida | ✅ completado |
| M3 | Repertorio, progreso e impresión | ✅ completado |
| M4 | Puntuación de dificultad | pendiente |
| M5 | Motor de transformación | pendiente |
| M6 | Asistencia por IA (opcional, desactivada por defecto) | pendiente |
| M7 | Importar y exportar Guitar Pro y MusicXML | pendiente |

## Cómo se escribe

Las manos no se sueltan del teclado. Los controles del vídeo están en las teclas de función para
no chocar con la escritura de notas.

| Teclas | Qué hacen |
|---|---|
| <kbd>↑</kbd> <kbd>↓</kbd> | cambiar de cuerda |
| <kbd>←</kbd> <kbd>→</kbd> | moverse por pulsos |
| <kbd>espacio</kbd> | avanzar |
| <kbd>0</kbd>–<kbd>9</kbd> | traste; dos dígitos seguidos dan del 10 al 24 |
| <kbd>+</kbd> <kbd>−</kbd> <kbd>.</kbd> | figura más corta, más larga, puntillo |
| <kbd>H</kbd> <kbd>V</kbd> <kbd>P</kbd> <kbd>G</kbd> <kbd>X</kbd> <kbd>A</kbd> <kbd>L</kbd> <kbd>S</kbd> | ligado, vibrato, apagado, fantasma, muerta, acento, dejar sonar, staccato |
| <kbd>R</kbd> | silencio |
| <kbd>Retroceso</kbd> / <kbd>⇧ Retroceso</kbd> | borrar nota / quitar el pulso entero |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd> / <kbd>Ctrl</kbd>+<kbd>S</kbd> | deshacer / guardar |
| <kbd>Ctrl</kbd>+<kbd>O</kbd> / <kbd>Ctrl</kbd>+<kbd>P</kbd> | abrir el repertorio / imprimir la hoja |
| <kbd>Intro</kbd> | sonar desde el compás en el que estás |
| <kbd>⇧</kbd>+<kbd>Intro</kbd> | repetir ese compás en bucle hasta volver a pulsar |
| <kbd>M</kbd> | metrónomo |
| clic en el mástil | escribe esa nota; el mismo traste otra vez la quita |
| <kbd>T</kbd> | marcar el compás como que se atraganta |
| <kbd>F1</kbd> <kbd>F2</kbd> <kbd>F3</kbd> <kbd>F4</kbd> | vídeo: reproducir, −3 s, media velocidad, bucle A–B |

El bucle A–B se marca con una sola tecla: una pulsación abre, otra cierra, otra lo quita.

La partitura suena desde donde estás trabajando, no desde el principio de la pieza:
quien está sacando el compás treinta quiere oír el treinta. Mientras suena, la rejilla
de escritura sigue a la música, así que al pausar te quedas escribiendo justo donde se
atascó la transcripción.

## Cómo está hecho

- **Rust** y **Tauri v2** para el núcleo y la ventana de escritorio.
- **TypeScript** y **[alphaTab](https://alphatab.net)** para el renderizado de partituras, el
  sintetizador y la impresión.
- Archivos **JSON locales** para los datos de práctica.

Las tablaturas se guardan como archivos JSON versionados en el propio repositorio: así hay copia
de seguridad, historial de cómo evolucionó cada arreglo, y publicarlas es simplemente un push.

Los datos de práctica — a qué velocidad me sale hoy, qué compases se me atragantan — se quedan en
archivos JSON locales que nunca se suben. Lo que se publica es la tablatura; cómo la llevo es mío.

## Instalación para desarrollo

Requisitos: [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 20 o superior, y en Windows
las Build Tools de Visual Studio con el SDK de Windows.

```bash
npm install
npm run tauri:dev
```

Para generar el ejecutable:

```bash
npm run tauri:build
```

Comprobaciones antes de enviar cambios:

```bash
npm run check              # formato, tipos y pruebas del frontend
npm run verify:alphatex    # ida y vuelta del serializador contra el propio alphaTab
cd src-tauri && cargo clippy --workspace --all-targets && cargo test --workspace
```

La verificación de AlphaTex es la que de verdad importa: genera casos desde Rust, los parsea con
la biblioteca que los va a leer y compara el resultado. Probar el serializador contra las propias
suposiciones no demuestra nada.

## Contribuciones

Se aceptan y se agradecen. Si transcribiste una tablatura que no existe en internet, mandarla aquí
es la mejor forma de que no se pierda: abre un pull request con el archivo JSON en `songs/`.

También son bienvenidas las correcciones de tablaturas existentes, las mejoras al motor de
transformación y los informes de errores. Si vas a meterte con algo grande, abre antes un issue
para conversarlo.

Convención del código: identificadores en inglés, comentarios y textos de interfaz en español.

## Licencia

MIT.

---

by renata
