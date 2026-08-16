/**
 * Traducción de teclas a intenciones de edición.
 *
 * Se separa del resto para poder probarla sin navegador ni partitura: dado un evento de
 * teclado, ¿qué quiso hacer la persona? Esa pregunta no necesita nada más.
 *
 * El diseño busca no soltar el teclado al transcribir: flechas para moverse, dígitos para
 * el traste, espacio para avanzar, y letras para las técnicas.
 */

/** Técnicas que se pueden activar con una tecla. Los bits coinciden con `NoteTechniques`. */
export const TECHNIQUE_BITS = {
  hammerPull: 1 << 0,
  ghost: 1 << 1,
  dead: 1 << 2,
  palmMute: 1 << 3,
  letRing: 1 << 4,
  staccato: 1 << 5,
  accent: 1 << 6,
  heavyAccent: 1 << 7,
  vibrato: 1 << 8,
  vibratoWide: 1 << 9,
} as const;

export type Intent =
  | { type: 'moveString'; delta: number }
  | { type: 'moveBeat'; delta: number }
  | { type: 'moveBar'; delta: number }
  | { type: 'digit'; value: number }
  | { type: 'advance' }
  | { type: 'clearString' }
  | { type: 'removeBeat' }
  | { type: 'insertBeat' }
  | { type: 'setRest' }
  | { type: 'changeDuration'; direction: 'longer' | 'shorter' }
  | { type: 'toggleDot' }
  | { type: 'toggleTechnique'; bit: number }
  | { type: 'undo' }
  | { type: 'redo' }
  | { type: 'play' };

/** Letras que activan técnicas. */
const TECHNIQUE_KEYS: Record<string, number> = {
  h: TECHNIQUE_BITS.hammerPull,
  v: TECHNIQUE_BITS.vibrato,
  p: TECHNIQUE_BITS.palmMute,
  g: TECHNIQUE_BITS.ghost,
  x: TECHNIQUE_BITS.dead,
  l: TECHNIQUE_BITS.letRing,
  a: TECHNIQUE_BITS.accent,
  s: TECHNIQUE_BITS.staccato,
};

/**
 * Interpreta un evento de teclado.
 *
 * Devuelve `null` si la tecla no significa nada aquí, para que el evento siga su curso
 * normal y no se rompan cosas como los atajos del sistema.
 */
export function interpret(event: KeyboardEvent): Intent | null {
  const ctrl = event.ctrlKey || event.metaKey;

  if (ctrl) {
    switch (event.key.toLowerCase()) {
      case 'z':
        return event.shiftKey ? { type: 'redo' } : { type: 'undo' };
      case 'y':
        return { type: 'redo' };
      default:
        return null;
    }
  }

  // Con Alt de por medio no interceptamos nada: son atajos del sistema.
  if (event.altKey) return null;

  switch (event.key) {
    case 'ArrowUp':
      // Hacia el agudo, que es bajar el número de cuerda.
      return { type: 'moveString', delta: -1 };
    case 'ArrowDown':
      return { type: 'moveString', delta: 1 };
    case 'ArrowLeft':
      return { type: 'moveBeat', delta: -1 };
    case 'ArrowRight':
      return { type: 'moveBeat', delta: 1 };
    case 'PageUp':
      return { type: 'moveBar', delta: -1 };
    case 'PageDown':
      return { type: 'moveBar', delta: 1 };
    case ' ':
      return { type: 'advance' };
    case 'Backspace':
    case 'Delete':
      return event.shiftKey ? { type: 'removeBeat' } : { type: 'clearString' };
    case 'Insert':
      return { type: 'insertBeat' };
    case '+':
      return { type: 'changeDuration', direction: 'shorter' };
    case '-':
      return { type: 'changeDuration', direction: 'longer' };
    case '.':
      return { type: 'toggleDot' };
    case 'Enter':
      return { type: 'play' };
    default:
      break;
  }

  if (event.key >= '0' && event.key <= '9') {
    return { type: 'digit', value: Number(event.key) };
  }

  if (event.key.toLowerCase() === 'r') {
    return { type: 'setRest' };
  }

  const technique = TECHNIQUE_KEYS[event.key.toLowerCase()];
  if (technique !== undefined) {
    return { type: 'toggleTechnique', bit: technique };
  }

  return null;
}

/**
 * Acumulador de dígitos para trastes de dos cifras.
 *
 * Sin esto no se puede escribir el traste 12: el primer `1` ya habría confirmado la nota.
 * Si el segundo dígito llega pronto y el número resultante cabe en el mástil, se combina;
 * si no, empieza una cifra nueva. Es como funciona Guitar Pro y es lo que la mano espera.
 */
export class FretAccumulator {
  private pending: number | null = null;
  private lastAt = 0;

  constructor(
    private readonly windowMs = 900,
    private readonly maxFret = 24,
  ) {}

  /**
   * Añade un dígito y devuelve el traste resultante.
   *
   * @param now Marca de tiempo, inyectable para poder probarlo sin esperar de verdad.
   */
  push(digit: number, now: number): number {
    const pending = this.pending;
    const withinWindow = pending !== null && now - this.lastAt <= this.windowMs;
    const combined = withinWindow ? pending * 10 + digit : digit;

    // Un `0` inicial no abre una cifra de dos: el traste 0 se confirma solo.
    if (withinWindow && combined <= this.maxFret) {
      this.pending = null;
      this.lastAt = now;
      return combined;
    }

    this.pending = digit === 0 ? null : digit;
    this.lastAt = now;
    return digit;
  }

  /** Olvida el dígito pendiente. Se llama al mover el cursor. */
  reset(): void {
    this.pending = null;
  }
}

/** Figuras ordenadas de larga a corta, para el cambio con `+` y `-`. */
export const DURATIONS = [1, 2, 4, 8, 16, 32, 64] as const;

/** Devuelve la figura vecina, sin salirse de los extremos. */
export function stepDuration(current: number, direction: 'longer' | 'shorter'): number {
  const index = DURATIONS.indexOf(current as (typeof DURATIONS)[number]);
  if (index === -1) return current;
  const next = direction === 'shorter' ? index + 1 : index - 1;
  return DURATIONS[Math.min(DURATIONS.length - 1, Math.max(0, next))] ?? current;
}
