/**
 * Búsqueda dentro del repertorio.
 *
 * Con veinte canciones sobra un desplegable; con doscientas, no. Buscar tiene que dar con
 * la canción por lo que uno recuerda de ella —un trozo del título, el grupo, o «blues»—
 * sin obligar a escribirlo como está guardado.
 *
 * Aquí no hay DOM a propósito: dado un repertorio y lo que se ha escrito, ¿qué canciones
 * quedan? Esa pregunta no necesita navegador y así se puede probar.
 */

/** Una canción del repertorio, tal y como la devuelve Rust. */
export interface SongCard {
  slug: string;
  title: string;
  artist: string | null;
  bar_count: number;
  tags: string[];
  tempo_bpm: number;
}

/**
 * Deja un texto en su forma comparable: sin acentos, sin mayúsculas.
 *
 * En español los acentos se escriben al guardar y se olvidan al buscar. Que «corazon»
 * encuentre «Mi corazón» no es un lujo: es lo que se espera.
 */
export function fold(text: string): string {
  return text
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/gu, '')
    .toLowerCase();
}

/**
 * Filtra el repertorio por título, artista o etiqueta.
 *
 * Con varias palabras tienen que aparecer todas, aunque sea en sitios distintos: «beatles
 * fingerstyle» busca lo que es de los Beatles *y* está etiquetado como fingerstyle, que es
 * lo que quiere decir quien escribe las dos palabras.
 */
export function filterSongs(songs: readonly SongCard[], query: string): SongCard[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...songs];

  return songs.filter((song) => {
    const haystack = fold([song.title, song.artist ?? '', ...song.tags].join(' '));
    return words.every((word) => haystack.includes(word));
  });
}
