/**
 * Autocomprobación del editor.
 *
 * Simula una sesión de tecleo real y comprueba lo que quedó en la partitura. Existe porque
 * el fallo que importa —que escribir con el teclado produzca la tablatura correcta— sólo
 * se manifiesta en la aplicación empaquetada, y ahí no hay forma de mirar el resultado
 * salvo dejándolo escrito en disco.
 */
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
        finalTex: editor.currentTex(),
      },
      null,
      2,
    ),
  );
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
