/**
 * Diapasón: el mástil dibujado debajo de la rejilla de escritura.
 *
 * El teclado sigue siendo lo rápido, pero hay notas que se colocan mirando la posición y
 * no contando trastes: un acorde que se busca con los dedos antes que con los números, o
 * una nota que no se sabe en qué traste cae pero sí dónde suena. Para eso está el mástil.
 *
 * Aquí sólo se dibuja. Qué hacer con el traste que se pulsa lo decide el editor, para que
 * esta parte se pueda probar sin navegador ni partitura.
 */

/** Hasta qué traste llega el mástil dibujado. Del 13 en adelante se escribe a teclado. */
export const LAST_FRET = 12;

/** Trastes con marca en el mástil de una guitarra. */
const SINGLE_INLAYS = [3, 5, 7, 9];

/** Marca del traste: ninguna, un punto, o el doble punto de la octava. */
export type Inlay = 'none' | 'single' | 'double';

/** Qué marca lleva un traste. Son las de cualquier guitarra, no una invención. */
export function inlayAt(fret: number): Inlay {
  if (fret === LAST_FRET) return 'double';
  return SINGLE_INLAYS.includes(fret) ? 'single' : 'none';
}

export interface FretboardView {
  stringCount: number;
  /** Nombres de las cuerdas al aire, de la 1ª a la última. */
  stringNames: readonly string[];
  /** Cuerda donde está el cursor: es la fila que se resalta. */
  cursorString: number;
  /** Trastes pisados en el pulso actual, por cuerda. */
  pressed: ReadonlyMap<number, number>;
}

/**
 * Dibuja el mástil.
 *
 * Devuelve HTML en vez de tocar el DOM para poder comprobarlo leyendo el resultado, que
 * es lo que hacen las pruebas.
 */
export function fretboardHtml(view: FretboardView): string {
  const rows: string[] = [];

  for (let string = 1; string <= view.stringCount; string += 1) {
    const pressed = view.pressed.get(string);
    const cells: string[] = [];

    for (let fret = 0; fret <= LAST_FRET; fret += 1) {
      const classes = ['fret'];
      if (fret === 0) classes.push('open');
      if (pressed === fret) classes.push('pressed');

      // El número sólo aparece donde hay nota: un mástil lleno de cifras deja de leerse
      // como un mástil y no dice nada que no diga ya la rejilla.
      const label = pressed === fret ? String(fret) : '';
      cells.push(
        `<button type="button" class="${classes.join(' ')}" data-string="${string}"` +
          ` data-fret="${fret}" title="cuerda ${string}, traste ${fret}">${label}</button>`,
      );
    }

    const rowClasses = ['neck-row'];
    if (string === view.cursorString) rowClasses.push('here');
    rows.push(
      `<div class="${rowClasses.join(' ')}" data-string="${string}">` +
        `<span class="open-name">${view.stringNames[string - 1] ?? ''}</span>` +
        cells.join('') +
        '</div>',
    );
  }

  // Regla de trastes: sólo los números que un guitarrista busca de verdad.
  const marks: string[] = ['<span class="open-name"></span>'];
  for (let fret = 0; fret <= LAST_FRET; fret += 1) {
    const inlay = inlayAt(fret);
    const label = inlay === 'none' && fret !== 0 ? '' : String(fret);
    marks.push(`<span class="fret-mark ${inlay}">${label}</span>`);
  }
  rows.push(`<div class="neck-row marks">${marks.join('')}</div>`);

  return `<div class="neck" style="--frets:${LAST_FRET + 1}">${rows.join('')}</div>`;
}
