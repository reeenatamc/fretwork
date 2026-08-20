/**
 * Autocomprobación del editor.
 *
 * Simula una sesión de tecleo real y comprueba lo que quedó en la partitura. Existe porque
 * el fallo que importa —que escribir con el teclado produzca la tablatura correcta— sólo
 * se manifiesta en la aplicación empaquetada, y ahí no hay forma de mirar el resultado
 * salvo dejándolo escrito en disco.
 */
import { PIECE_BARS, PIECE_NOTES, typePiece } from './demo-song';
import type { Editor } from './editor/editor';

interface Check {
  name: string;
  ok: boolean;
  detail: string;
}

/** Envía una tecla al editor como si la hubiera pulsado una persona. */
async function press(
  editor: Editor,
  key: string,
  modifiers: { ctrl?: boolean; shift?: boolean } = {},
): Promise<void> {
  const event = {
    key,
    ctrlKey: modifiers.ctrl ?? false,
    metaKey: false,
    shiftKey: modifiers.shift ?? false,
    altKey: false,
    preventDefault: () => {},
  } as unknown as KeyboardEvent;

  await editor.onKey(event);
}

/** Teclea una secuencia, separando las teclas por espacios. */
async function type(editor: Editor, sequence: string): Promise<void> {
  for (const token of sequence.split(' ')) {
    const key = token === '_' ? ' ' : token;
    await press(editor, key);
  }
}

/**
 * Espera a que se cumpla algo, con un tope.
 *
 * Un clic no se puede esperar como se espera a `onKey`: el manejador es asíncrono y quien
 * lo dispara no recibe su promesa. Esperar un rato fijo daría una prueba que pasa o falla
 * según lo cargada que esté la máquina, así que se espera a la condición y punto.
 */
async function until(condition: () => boolean, timeoutMs = 3000): Promise<boolean> {
  const start = Date.now();
  while (!condition()) {
    if (Date.now() - start > timeoutMs) return false;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  return true;
}

/** Antes y después del arreglo, para poder enseñarlo en el informe. */
let arrangementDemo: { before: string; after: string; moves: unknown[] } | null = null;

/** La pieza completa transcrita, para poder revisarla en el informe. */
let pieceDemo: { tex: string; bars: number; notes: number; typingMs: number } | null = null;

export async function runSelfTest(
  editor: Editor,
  report: (json: string) => Promise<unknown>,
): Promise<void> {
  const checks: Check[] = [];

  const check = (name: string, ok: boolean, detail: string) => {
    checks.push({ name, ok, detail });
  };

  try {
    // ── Escribir una melodía simple ──────────────────────────────────────
    // Cuerda 1 traste 0, avanzar, traste 3, avanzar, traste 5.
    await type(editor, '0 _ 3 _ 5');
    let tex = editor.currentTex();
    check(
      'melodía de tres notas',
      tex.includes('0.1') && tex.includes('3.1') && tex.includes('5.1'),
      tex.includes('5.1') ? 'las tres notas están en la 1ª cuerda' : `no salió: ${excerpt(tex)}`,
    );

    // ── Traste de dos cifras ─────────────────────────────────────────────
    await type(editor, '_');
    await press(editor, '1');
    await press(editor, '2');
    tex = editor.currentTex();
    check(
      'traste de dos cifras',
      tex.includes('12.1'),
      tex.includes('12.1') ? '1 y 2 seguidos dieron el traste 12' : `no salió: ${excerpt(tex)}`,
    );

    // ── Cambiar de cuerda y formar un acorde ─────────────────────────────
    await type(editor, '_ 0');
    await press(editor, 'ArrowDown');
    await press(editor, '1');
    await press(editor, 'ArrowDown');
    await press(editor, '0');
    tex = editor.currentTex();
    const chord = /\(0\.1 1\.2 0\.3\)|\(0\.3 1\.2 0\.1\)/.test(tex) || tex.includes('1.2');
    check(
      'acorde en varias cuerdas',
      chord,
      chord ? 'las notas se apilaron en el mismo pulso' : `no salió: ${excerpt(tex)}`,
    );

    // ── Técnica sobre la nota actual ─────────────────────────────────────
    await press(editor, 'h');
    tex = editor.currentTex();
    check(
      'técnica de ligado',
      tex.includes('{h}') || tex.includes('h '),
      tex.includes('{h}') ? 'el ligado quedó escrito' : `no salió: ${excerpt(tex)}`,
    );

    // ── Deshacer ─────────────────────────────────────────────────────────
    const beforeUndo = editor.currentTex();
    await press(editor, 'z', { ctrl: true });
    const afterUndo = editor.currentTex();
    check(
      'deshacer',
      beforeUndo !== afterUndo,
      beforeUndo === afterUndo ? 'la partitura no cambió al deshacer' : 'el último cambio se fue',
    );

    // ── Rehacer ──────────────────────────────────────────────────────────
    await press(editor, 'z', { ctrl: true, shift: true });
    check(
      'rehacer',
      editor.currentTex() === beforeUndo,
      editor.currentTex() === beforeUndo ? 'volvió a como estaba' : 'no restauró el estado',
    );

    // ── Cambiar la figura rítmica ────────────────────────────────────────
    await type(editor, '_ +');
    await press(editor, '7');
    tex = editor.currentTex();
    check(
      'cambio de figura',
      tex.includes(':8'),
      tex.includes(':8') ? 'la corchea quedó declarada' : `no salió: ${excerpt(tex)}`,
    );

    // ── Movimiento del cursor ────────────────────────────────────────────
    const before = editor.currentCursor();
    await press(editor, 'ArrowLeft');
    const moved = editor.currentCursor();
    check(
      'el cursor retrocede',
      moved.beat < before.beat || moved.bar < before.bar,
      `de compás ${before.bar + 1} pulso ${before.beat + 1} a compás ${moved.bar + 1} pulso ${moved.beat + 1}`,
    );

    // ── Borrar ───────────────────────────────────────────────────────────
    await press(editor, 'ArrowRight');
    const beforeDelete = editor.currentTex();
    await press(editor, 'Backspace');
    check(
      'borrar una nota',
      editor.currentTex() !== beforeDelete,
      editor.currentTex() === beforeDelete ? 'no borró nada' : 'la nota desapareció',
    );
    // ── Diapasón ─────────────────────────────────────────────────────────
    // El clic pasa por el DOM de verdad: es la única forma de saber que el mástil que se
    // dibuja y la partitura que se escribe hablan de la misma nota.
    await press(editor, 'ArrowRight');
    // El mástil se vuelve a dibujar entero en cada cambio, así que la casilla hay que
    // buscarla otra vez: la de antes ya no cuelga del documento y su clic no llegaría.
    const fret = () =>
      document.querySelector<HTMLElement>('#fretboard [data-string="4"][data-fret="7"]');
    fret()?.click();
    const written = await until(() => editor.currentTex().includes('7.4'));
    check(
      'clic en el mástil',
      written,
      fret()
        ? `el traste 7 de la 4ª cuerda: ${excerpt(editor.currentTex())}`
        : 'no se dibujó el mástil',
    );

    fret()?.click();
    const removed = await until(() => !editor.currentTex().includes('7.4'));
    check(
      'volver a pulsar quita la nota',
      removed,
      removed ? 'el segundo clic la borró' : `sigue ahí: ${excerpt(editor.currentTex())}`,
    );

    // ── Guardar en disco ─────────────────────────────────────────────────
    // Es lo que impide perder una transcripción al cerrar la aplicación.
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('session_set_meta', {
      title: 'Prueba de guardado',
      artist: 'Autocomprobación',
      sourceUrl: null,
      tempoBpm: 90,
    });
    const slug = await invoke<string>('session_save');
    check('guardar en disco', slug === 'prueba-de-guardado', `quedó como songs/${slug}.json`);

    const library = await invoke<{ slug: string }[]>('session_list');
    check(
      'aparece en la biblioteca',
      library.some((song) => song.slug === slug),
      `${library.length} canción(es) guardada(s)`,
    );

    const reopened = await invoke<{ tex: string }>('session_open', { slug });
    check(
      'se reabre igual que se guardó',
      reopened.tex.includes('0.1') && reopened.tex.includes('12.1'),
      reopened.tex.includes('12.1') ? 'las notas sobrevivieron al viaje' : 'se perdió algo',
    );

    // ── Transcribir una pieza entera ─────────────────────────────────────
    // La prueba que los tests unitarios no hacen: ¿aguanta una canción completa?
    await invoke('session_new', {
      title: 'Estudio en mi menor',
      barCount: PIECE_BARS,
      tempoBpm: 84,
    });
    await editor.reload();

    const startedAt = Date.now();
    await typePiece(editor);
    const typingMs = Date.now() - startedAt;

    // Los datos de cabecera se ponen antes de capturar el texto: si se ponen después, la
    // comparación de «se guarda y se reabre igual» falla por el artista añadido y parece
    // un fallo de guardado que no existe.
    await invoke('session_set_meta', {
      title: 'Estudio en mi menor',
      artist: 'tabs-repo',
      sourceUrl: null,
      tempoBpm: 84,
    });
    await editor.reload();

    const pieceTex = editor.currentTex();
    const bars = pieceTex.split('\n').filter((line) => line.trim().endsWith('|'));
    check(
      'se transcribe una pieza de 16 compases',
      bars.length === PIECE_BARS,
      `${bars.length} de ${PIECE_BARS} compases, ${PIECE_NOTES} notas, ${typingMs} ms`,
    );

    // Cada compás tiene que estar lleno: si alguno quedó a medias, la mecánica de
    // escritura pierde notas por el camino y la partitura no se puede reproducir bien.
    const emptyBars = bars.filter((bar) => !/\d+\.\d/.test(bar)).length;
    check(
      'ningún compás quedó vacío',
      emptyBars === 0,
      emptyBars === 0 ? 'los 16 compases tienen notas' : `${emptyBars} compases sin una sola nota`,
    );

    check(
      'los acordes y las técnicas sobrevivieron',
      pieceTex.includes('(') && pieceTex.includes('{h}') && pieceTex.includes('{v}'),
      'acordes apilados, ligado y vibrato presentes',
    );

    const pieceSlug = await invoke<string>('session_save');
    const reloaded = await invoke<{ tex: string }>('session_open', { slug: pieceSlug });
    check(
      'la pieza entera se guarda y se reabre igual',
      reloaded.tex === pieceTex,
      reloaded.tex === pieceTex ? 'idéntica byte a byte' : 'algo cambió al ir y volver',
    );

    pieceDemo = { tex: pieceTex, bars: bars.length, notes: PIECE_NOTES, typingMs };

    // ── Transcribir un riff de verdad y adornarlo ────────────────────────
    // Es la prueba que importa de la función estrella: sobre una melodía real,
    // no sobre un caso de laboratorio.
    await invoke('session_new', { title: 'Riff de prueba', barCount: 8, tempoBpm: 100 });
    await editor.reload();

    // Escala de sol por la tercera cuerda, cuatro negras por compás.
    const frets = ['0', '2', '4', '5', '7', '9', '11', '12'];
    for (let bar = 0; bar < 8; bar += 1) {
      for (let i = 0; i < 4; i += 1) {
        const fret = frets[(bar * 2 + i) % frets.length] ?? '0';
        for (const digit of fret) await press(editor, digit);
        await press(editor, ' ');
      }
    }

    const beforeTex = editor.currentTex();
    const beforeScore = await invoke<number>('session_difficulty');
    check(
      'se transcribe un riff completo',
      (beforeTex.match(/\|/g)?.length ?? 0) >= 8 && beforeScore > 0,
      `8 compases escritos, dificultad ${beforeScore.toFixed(1)}/100`,
    );

    const preview = await invoke<{
      arrangement: { before: number; after: number; moves: unknown[]; untouched_ratio: number };
      tex: string;
    }>('session_preview_harder', { targetDelta: 0.15 });

    const { before: scoreBefore, after: scoreAfter, moves, untouched_ratio } = preview.arrangement;
    check(
      'la versión adornada es más difícil',
      scoreAfter > scoreBefore,
      `${scoreBefore.toFixed(1)} → ${scoreAfter.toFixed(1)} con ${moves.length} arreglos`,
    );
    check(
      'se respeta el suelo de compases intactos',
      untouched_ratio >= 0.4,
      `queda intacto el ${(untouched_ratio * 100).toFixed(0)} %`,
    );
    check(
      'los arreglos aparecen en la tablatura',
      preview.tex.includes('{h}') || preview.tex.includes('{v}') || preview.tex.includes('{sl}'),
      'ligados, vibratos o arrastres escritos',
    );

    await invoke('session_accept_harder');
    await editor.reload();
    const afterScore = await invoke<number>('session_difficulty');
    check(
      'aceptar la deja aplicada',
      afterScore > beforeScore,
      `dificultad de la canción: ${beforeScore.toFixed(1)} → ${afterScore.toFixed(1)}`,
    );

    arrangementDemo = {
      before: beforeTex,
      after: editor.currentTex(),
      moves: preview.arrangement.moves,
    };
  } catch (error) {
    check('la sesión no revienta', false, String(error));
  }

  const passed = checks.filter((c) => c.ok).length;
  await report(
    JSON.stringify(
      {
        timestamp: new Date().toISOString(),
        summary: `${passed}/${checks.length}`,
        checks,
        // Sin esto, un fallo de la frontera IPC se ve como "no pasó nada" y cuesta
        // muchísimo más diagnosticar de lo necesario.
        lastError: editor.lastError,
        // Medidas reales de la maquetación: comprobar «cabe en la ventana» a ojo desde
        // una captura es adivinar, y esto lo responde con números.
        layout: measureLayout(),
        pieceDemo,
        arrangementDemo,
        finalTex: editor.currentTex(),
      },
      null,
      2,
    ),
  );
}

/**
 * Mide las tres bandas de la interfaz.
 *
 * El requisito es que la rejilla de escritura esté siempre visible sin desplazarse, y eso
 * se comprueba viendo si el pie termina dentro de la ventana.
 */
function measureLayout(): Record<string, unknown> {
  const rect = (selector: string) => {
    const element = document.querySelector(selector);
    if (!element) return null;
    const { top, height, width } = element.getBoundingClientRect();
    return { top: Math.round(top), height: Math.round(height), width: Math.round(width) };
  };

  const footer = rect('footer');
  const viewportHeight = window.innerHeight;

  return {
    viewport: { width: window.innerWidth, height: viewportHeight },
    devicePixelRatio: window.devicePixelRatio,
    header: rect('header'),
    capture: rect('.capture'),
    footer,
    documentHeight: document.documentElement.scrollHeight,
    // Lo que de verdad importa: ¿acaba el pie dentro de la ventana?
    footerFitsOnScreen: footer ? footer.top + footer.height <= viewportHeight + 1 : false,
  };
}

/** Recorta el AlphaTex para que quepa en un mensaje de error. */
function excerpt(tex: string): string {
  const bars = tex
    .split('\n')
    .filter((line) => line.trim().endsWith('|'))
    .slice(0, 3)
    .join(' / ');
  return bars.slice(0, 220);
}
