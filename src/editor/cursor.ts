/**
 * Cursor de edición: dónde está la mano dentro de la partitura.
 *
 * Se mueve por compás, pulso y cuerda. La cuerda 1 es la más aguda, así que la flecha
 * arriba baja el número de cuerda — que es lo que espera quien mira un diapasón.
 */

export interface Cursor {
  bar: number;
  /** Índice del pulso dentro del compás. */
  beat: number;
  /** Cuerda, donde 1 es la más aguda. */
  string: number;
  voice: number;
}

export interface CursorBounds {
  barCount: number;
  stringCount: number;
  /** Cuántos pulsos tiene cada compás ahora mismo. */
  beatsPerBar: (bar: number) => number;
}

export function createCursor(): Cursor {
  return { bar: 0, beat: 0, string: 1, voice: 0 };
}

/** Convierte el cursor en la dirección que entiende Rust. */
export function toAddr(cursor: Cursor) {
  return {
    track: 0,
    staff: 0,
    bar: cursor.bar,
    voice: cursor.voice,
    beat: cursor.beat,
  };
}

/**
 * Sube o baja de cuerda.
 *
 * `delta` negativo va hacia el agudo (cuerda 1). No da la vuelta al llegar al borde:
 * al transcribir, que el cursor salte de la prima al bordón sin avisar desorienta.
 */
export function moveString(cursor: Cursor, delta: number, bounds: CursorBounds): Cursor {
  const string = Math.min(bounds.stringCount, Math.max(1, cursor.string + delta));
  return { ...cursor, string };
}

/**
 * Avanza o retrocede pulsos, cruzando de compás cuando hace falta.
 *
 * Al pasar al compás siguiente cae en su primer pulso; al retroceder, en el último del
 * anterior. Avanzar más allá del final se queda en el último compás.
 */
export function moveBeat(cursor: Cursor, delta: number, bounds: CursorBounds): Cursor {
  let { bar, beat } = cursor;
  let remaining = delta;

  while (remaining > 0) {
    const inBar = Math.max(1, bounds.beatsPerBar(bar));
    if (beat + 1 < inBar) {
      beat += 1;
    } else if (bar + 1 < bounds.barCount) {
      bar += 1;
      beat = 0;
    } else {
      // Último compás: se permite un pulso más para poder seguir escribiendo.
      beat += 1;
    }
    remaining -= 1;
  }

  while (remaining < 0) {
    if (beat > 0) {
      beat -= 1;
    } else if (bar > 0) {
      bar -= 1;
      beat = Math.max(0, bounds.beatsPerBar(bar) - 1);
    }
    remaining += 1;
  }

  return { ...cursor, bar, beat };
}

/** Salta al principio de un compás concreto. */
export function moveToBar(cursor: Cursor, bar: number, bounds: CursorBounds): Cursor {
  const clamped = Math.min(bounds.barCount - 1, Math.max(0, bar));
  return { ...cursor, bar: clamped, beat: 0 };
}
