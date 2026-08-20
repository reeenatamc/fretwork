/**
 * El progreso de una canción, del lado de la interfaz.
 *
 * Igual que la búsqueda, esto es cálculo puro: dado el progreso guardado y el tempo de la
 * grabación, ¿qué se enseña? Sin DOM, para poder probarlo.
 */

/** En qué punto está una canción. Coincide con el enum de Rust. */
export type Status = 'sacando' | 'ensayando' | 'lista';

export interface Practice {
  status: Status;
  /** A qué velocidad sale hoy. Cero mientras no se haya medido. */
  tempo_bpm: number;
  /** A qué velocidad tiene que salir. Cero significa «la de la grabación». */
  target_bpm: number;
  /** Compases que siguen tropezando. */
  tricky_bars: number[];
}

/** Progreso de una canción con la canción a la que pertenece. */
export interface PracticeEntry {
  slug: string;
  practice: Practice;
}

/** Cómo se llama cada estado en la interfaz. */
export const STATUS_LABEL: Record<Status, string> = {
  sacando: 'sacándola',
  ensayando: 'ensayando',
  lista: 'lista',
};

/** El progreso de una canción que todavía no se ha ensayado. */
export function emptyPractice(): Practice {
  return { status: 'sacando', tempo_bpm: 0, target_bpm: 0, tricky_bars: [] };
}

/**
 * A qué velocidad tiene que salir.
 *
 * Si no se ha puesto un objetivo, el objetivo es la grabación: es lo que se quiere tocar.
 */
export function targetTempo(practice: Practice, songTempo: number): number {
  return practice.target_bpm > 0 ? practice.target_bpm : songTempo;
}

/**
 * Lo cerca que está de tocarla a tempo, de 0 a 1.
 *
 * Es la medida que dice qué toca ensayar hoy, y por eso no se redondea a «hecha» o «sin
 * hacer»: la diferencia entre el 60 % y el 90 % del tempo son semanas de trabajo.
 */
export function tempoRatio(practice: Practice, songTempo: number): number {
  const target = targetTempo(practice, songTempo);
  if (target <= 0 || practice.tempo_bpm <= 0) return 0;
  return Math.min(1, practice.tempo_bpm / target);
}

/**
 * Cómo se lee el tempo en el repertorio.
 *
 * Sin medir no se inventa un número: se dice que no se ha medido, que es distinto de ir
 * a cero.
 */
export function tempoLabel(practice: Practice, songTempo: number): string {
  const target = targetTempo(practice, songTempo);
  if (practice.tempo_bpm <= 0) return `sin medir · ${Math.round(target)} BPM`;
  const percent = Math.round(tempoRatio(practice, songTempo) * 100);
  return `${Math.round(practice.tempo_bpm)} de ${Math.round(target)} BPM · ${percent} %`;
}
