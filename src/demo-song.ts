/**
 * Transcripción completa de una pieza, tecleada como lo haría una persona.
 *
 * Existe para responder a la única pregunta que los tests unitarios no responden: ¿aguanta
 * la aplicación una canción entera de principio a fin? Los fallos que importan —el compás
 * que no avanzaba, la figura que se arrastraba entre canciones— sólo aparecieron
 * escribiendo de verdad, no probando piezas por separado.
 *
 * La pieza es un estudio propio en mi menor, no una canción conocida: al definirla nota a
 * nota se puede comprobar que lo que sale es exactamente lo que entró. Con una
 * transcripción de oído no habría forma de distinguir un fallo de la aplicación de un
 * error mío al recordarla.
 */

import type { Editor } from './editor/editor';

/** Una nota: cuerda (1 = prima), traste, y técnicas opcionales por tecla. */
interface Note {
  string: number;
  fret: number;
  keys?: string;
}

/** Un pulso: una o varias notas a la vez, con su figura. */
interface Beat {
  notes: Note[];
  /** Figura: 4 negra, 8 corchea. Si no se indica, sigue la anterior. */
  duration?: number;
}

/**
 * Estudio en mi menor, 16 compases en 4/4.
 *
 * Estructura: A (Em–C) cuatro compases, A' (G–D) cuatro, B con la melodía en la prima
 * cuatro, y vuelta a A con cierre. Bajo alternado en las cuerdas graves y melodía en las
 * agudas, que es como se toca fingerstyle.
 */
const PIECE: Beat[][] = [
  // ── A: Em ────────────────────────────────────────────────────────────────
  // Compás 1: bajo mi, arpegio ascendente.
  [
    { notes: [{ string: 6, fret: 0 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 1, fret: 0 }] },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 3, fret: 0 }] },
    { notes: [{ string: 2, fret: 0 }] },
  ],
  // Compás 2: igual con el quinto bajo.
  [
    { notes: [{ string: 5, fret: 2 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 1, fret: 2 }] },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 3, fret: 0 }] },
    { notes: [{ string: 2, fret: 3 }] },
  ],
  // Compás 3: Do mayor.
  [
    { notes: [{ string: 5, fret: 3 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 1 }] },
    { notes: [{ string: 1, fret: 0 }] },
    { notes: [{ string: 2, fret: 1 }] },
    { notes: [{ string: 3, fret: 0 }] },
    { notes: [{ string: 2, fret: 1 }] },
  ],
  // Compás 4: cierre de la frase sobre el acorde entero.
  [
    { notes: [{ string: 5, fret: 3 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 1 }] },
    // Blanca, no negra: con una negra el compás sumaría 3/4 y el contenido del siguiente
    // se derramaría dentro.
    {
      notes: [
        { string: 3, fret: 0 },
        { string: 2, fret: 1 },
        { string: 1, fret: 0 },
      ],
      duration: 2,
    },
  ],

  // ── A': Sol – Re ─────────────────────────────────────────────────────────
  [
    { notes: [{ string: 6, fret: 3 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 1, fret: 3 }] },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 3, fret: 0 }] },
    { notes: [{ string: 2, fret: 0 }] },
  ],
  [
    { notes: [{ string: 6, fret: 3 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 1, fret: 2 }] },
    { notes: [{ string: 1, fret: 0 }] },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 3, fret: 0 }] },
  ],
  [
    { notes: [{ string: 4, fret: 0 }], duration: 4 },
    { notes: [{ string: 3, fret: 2 }], duration: 8 },
    { notes: [{ string: 2, fret: 3 }] },
    { notes: [{ string: 1, fret: 2 }] },
    { notes: [{ string: 2, fret: 3 }] },
    { notes: [{ string: 3, fret: 2 }] },
    { notes: [{ string: 2, fret: 3 }] },
  ],
  [
    { notes: [{ string: 4, fret: 0 }], duration: 4 },
    { notes: [{ string: 3, fret: 2 }], duration: 8 },
    { notes: [{ string: 2, fret: 3 }] },
    {
      notes: [
        { string: 3, fret: 2 },
        { string: 2, fret: 3 },
        { string: 1, fret: 2 },
      ],
      duration: 2,
    },
  ],

  // ── B: melodía en la prima, con ligados ──────────────────────────────────
  [
    { notes: [{ string: 6, fret: 0 }], duration: 4 },
    { notes: [{ string: 1, fret: 0 }], duration: 8 },
    { notes: [{ string: 1, fret: 2, keys: 'h' }] },
    { notes: [{ string: 1, fret: 3 }] },
    { notes: [{ string: 1, fret: 2 }] },
    { notes: [{ string: 2, fret: 3 }] },
    { notes: [{ string: 2, fret: 0 }] },
  ],
  [
    { notes: [{ string: 5, fret: 2 }], duration: 4 },
    { notes: [{ string: 2, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 1 }] },
    { notes: [{ string: 2, fret: 3 }] },
    { notes: [{ string: 1, fret: 0 }] },
    { notes: [{ string: 1, fret: 2 }] },
    { notes: [{ string: 1, fret: 3, keys: 'v' }] },
  ],
  [
    { notes: [{ string: 5, fret: 3 }], duration: 4 },
    { notes: [{ string: 1, fret: 5 }], duration: 8 },
    { notes: [{ string: 1, fret: 3 }] },
    { notes: [{ string: 1, fret: 2 }] },
    { notes: [{ string: 1, fret: 0 }] },
    { notes: [{ string: 2, fret: 3 }] },
    { notes: [{ string: 2, fret: 1 }] },
  ],
  [
    { notes: [{ string: 4, fret: 0 }], duration: 4 },
    { notes: [{ string: 2, fret: 0 }], duration: 8 },
    { notes: [{ string: 3, fret: 2 }] },
    { notes: [{ string: 3, fret: 0 }], duration: 4 },
    { notes: [], duration: 4 },
  ],

  // ── Vuelta a A y cierre ──────────────────────────────────────────────────
  [
    { notes: [{ string: 6, fret: 0 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 1, fret: 0 }] },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 3, fret: 0 }] },
    { notes: [{ string: 2, fret: 0 }] },
  ],
  [
    { notes: [{ string: 5, fret: 2 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 1, fret: 2 }] },
    { notes: [{ string: 2, fret: 0 }] },
    { notes: [{ string: 3, fret: 0 }] },
    { notes: [{ string: 2, fret: 3 }] },
  ],
  [
    { notes: [{ string: 5, fret: 3 }], duration: 4 },
    { notes: [{ string: 3, fret: 0 }], duration: 8 },
    { notes: [{ string: 2, fret: 1 }] },
    { notes: [{ string: 1, fret: 0 }] },
    { notes: [{ string: 2, fret: 1 }] },
    { notes: [{ string: 3, fret: 0 }] },
    { notes: [{ string: 2, fret: 1 }] },
  ],
  // Compás final: el acorde de mi menor completo, redonda.
  [
    {
      notes: [
        { string: 6, fret: 0 },
        { string: 5, fret: 2 },
        { string: 4, fret: 2 },
        { string: 3, fret: 0 },
        { string: 2, fret: 0 },
        { string: 1, fret: 0 },
      ],
      duration: 1,
    },
  ],
];

/** Cuántos compases tiene la pieza. */
export const PIECE_BARS = PIECE.length;

/** Cuántas notas se escriben en total, para comprobar que no se pierde ninguna. */
export const PIECE_NOTES = PIECE.reduce(
  (total, bar) => total + bar.reduce((sum, beat) => sum + beat.notes.length, 0),
  0,
);

/** Envía una tecla al editor como si la hubiera pulsado una persona. */
async function press(editor: Editor, key: string): Promise<void> {
  await editor.onKey({
    key,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    preventDefault: () => {},
  } as unknown as KeyboardEvent);
}

/**
 * Teclea la pieza entera.
 *
 * Reproduce la mecánica real de transcribir: elegir figura, moverse de cuerda, escribir el
 * traste, apilar las notas de un acorde y avanzar. No hay atajos por debajo: todo pasa por
 * el mismo camino que usa el teclado.
 */
export async function typePiece(editor: Editor): Promise<void> {
  let currentString = 1;
  let currentDuration = 4;

  for (const bar of PIECE) {
    for (const beat of bar) {
      // La figura se ajusta subiendo o bajando, igual que con las teclas + y −.
      if (beat.duration && beat.duration !== currentDuration) {
        const shorter = beat.duration > currentDuration;
        const steps = Math.abs(Math.log2(beat.duration) - Math.log2(currentDuration));
        for (let i = 0; i < steps; i += 1) {
          await press(editor, shorter ? '+' : '-');
        }
        currentDuration = beat.duration;
      }

      if (beat.notes.length === 0) {
        await press(editor, 'r');
      }

      for (const note of beat.notes) {
        // Moverse a la cuerda: hacia el agudo baja el número.
        while (currentString > note.string) {
          await press(editor, 'ArrowUp');
          currentString -= 1;
        }
        while (currentString < note.string) {
          await press(editor, 'ArrowDown');
          currentString += 1;
        }

        for (const digit of String(note.fret)) {
          await press(editor, digit);
        }
        for (const key of note.keys ?? '') {
          await press(editor, key);
        }
      }

      await press(editor, ' ');
    }
  }
}
