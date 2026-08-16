/**
 * Arranque de la aplicación: editor, reproductor de YouTube y guardado.
 *
 * Si el proceso se lanzó con `TABS_SELFTEST=1`, ejecuta una sesión de tecleo guionizada y
 * deja el resultado en disco: es la forma de comprobar que la captura funciona en la build
 * empaquetada sin depender de que alguien mire la ventana.
 */
import { Editor } from './editor/editor';
import { extractVideoId, YouTubePlayer } from './player/youtube';
import { runSelfTest } from './selftest';

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

interface SongEntry {
  slug: string;
  title: string;
  artist: string | null;
  bar_count: number;
}

async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
  return tauriInvoke<T>(command, args);
}

let editor: Editor;
let player: YouTubePlayer | null = null;
/** Extremo A del bucle. `null` mientras no se haya marcado. */
let loopStart: number | null = null;

// ─────────────────────────────────────────────────────────── Cabecera

function notify(message: string): void {
  $('saved').textContent = message;
  window.setTimeout(() => {
    if ($('saved').textContent === message) $('saved').textContent = '';
  }, 4000);
}

async function pushMeta(): Promise<void> {
  await invoke('session_set_meta', {
    title: $<HTMLInputElement>('title').value.trim() || 'Sin título',
    artist: $<HTMLInputElement>('artist').value.trim() || null,
    sourceUrl: $<HTMLInputElement>('yt-id').value.trim() || null,
    tempoBpm: Number($<HTMLInputElement>('bpm').value) || 90,
  });
}

async function save(): Promise<void> {
  try {
    await pushMeta();
    const slug = await invoke<string>('session_save');
    notify(`guardado en songs/${slug}.json`);
    await refreshLibrary();
  } catch (error) {
    notify(`⚠ ${formatError(error)}`);
  }
}

async function refreshLibrary(): Promise<void> {
  try {
    const songs = await invoke<SongEntry[]>('session_list');
    const select = $<HTMLSelectElement>('library');
    const current = select.value;
    select.innerHTML =
      '<option value="">— abrir —</option>' +
      songs
        .map(
          (song) =>
            `<option value="${song.slug}">${song.title}${song.artist ? ` — ${song.artist}` : ''}</option>`,
        )
        .join('');
    select.value = current;
  } catch {
    // Si el listado falla no pasa nada grave: se sigue pudiendo escribir y guardar.
  }
}

// ─────────────────────────────────────────────────────────── YouTube

async function loadVideo(): Promise<void> {
  const raw = $<HTMLInputElement>('yt-id').value.trim();
  const videoId = extractVideoId(raw);
  if (!videoId) {
    $('yt-status').textContent = 'No reconozco ese enlace.';
    return;
  }

  player?.destroy();
  loopStart = null;
  $('yt-status').textContent = 'Cargando…';

  // YT.Player reemplaza el elemento que recibe, así que se le da un hijo desechable.
  const host = $('yt-host');
  host.innerHTML = '';
  const mount = document.createElement('div');
  host.appendChild(mount);

  player = new YouTubePlayer(mount, {
    onError: (_code, message) => {
      $('yt-status').textContent = `⚠ ${message}`;
    },
    onTime: (seconds) => {
      const loop = player?.getLoop();
      $('yt-status').textContent =
        `${format(seconds)} / ${format(player?.getDuration() ?? 0)}` +
        `   ${player?.getPlaybackRate() ?? 1}×` +
        (loop ? `   bucle ${format(loop.start)}–${format(loop.end)}` : '');
    },
  });

  try {
    await player.load(videoId);
    $('yt-status').textContent = 'Listo.';
  } catch (error) {
    $('yt-status').textContent = `⚠ ${formatError(error)}`;
  }
}

/**
 * Marca los extremos del bucle con una sola tecla.
 *
 * La primera pulsación fija el principio, la segunda el final y arranca el bucle, la
 * tercera lo quita. Al transcribir se repite un fragmento decenas de veces, y tener que
 * apuntar dos tiempos a mano cada vez sería insufrible.
 */
function toggleLoop(): void {
  if (!player) return;

  if (player.getLoop()) {
    player.setLoop(null);
    loopStart = null;
    return;
  }

  if (loopStart === null) {
    loopStart = player.getCurrentTime();
    $('yt-status').textContent = `bucle desde ${format(loopStart)} — pulsa otra vez para cerrar`;
    return;
  }

  const end = player.getCurrentTime();
  if (end > loopStart) {
    player.setLoop({ start: loopStart, end });
  }
  loopStart = null;
}

function format(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = Math.floor(seconds % 60);
  return `${minutes}:${String(rest).padStart(2, '0')}`;
}

function formatError(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object') {
    const values = Object.values(error as Record<string, unknown>);
    if (values.length === 1 && typeof values[0] === 'string') return values[0];
    return JSON.stringify(error);
  }
  return String(error);
}

// ─────────────────────────────────────────────────────────── Arranque

async function main(): Promise<void> {
  if (!('__TAURI_INTERNALS__' in window)) {
    $('status').textContent = 'Abre la aplicación con `npm run tauri:dev`.';
    return;
  }

  editor = new Editor($('score'), $('grid'), $('status'));
  await editor.start('Sin título', 16, 90);

  $('save').addEventListener('click', () => void save());
  $('library').addEventListener('change', async (event) => {
    const slug = (event.target as HTMLSelectElement).value;
    if (!slug) return;
    try {
      await invoke('session_open', { slug });
      await editor.reload();
      notify(`abierto ${slug}`);
    } catch (error) {
      notify(`⚠ ${formatError(error)}`);
    }
  });

  $('yt-load').addEventListener('click', () => void loadVideo());
  $('yt-play').addEventListener('click', () => player?.toggle());
  $('yt-rewind').addEventListener('click', () => player?.rewind(3));
  $('yt-slow').addEventListener('click', () => player?.setPlaybackRate(0.5));
  $('yt-075').addEventListener('click', () => player?.setPlaybackRate(0.75));
  $('yt-normal').addEventListener('click', () => player?.setPlaybackRate(1));
  $('yt-loop').addEventListener('click', toggleLoop);

  // Atajos del reproductor que no chocan con la escritura de notas.
  window.addEventListener('keydown', (event) => {
    if (event.ctrlKey || event.metaKey) {
      if (event.key.toLowerCase() === 's') {
        event.preventDefault();
        void save();
      }
      return;
    }
    switch (event.key) {
      case 'F1':
        event.preventDefault();
        player?.toggle();
        break;
      case 'F2':
        event.preventDefault();
        player?.rewind(3);
        break;
      case 'F3':
        event.preventDefault();
        player?.setPlaybackRate(player.getPlaybackRate() === 1 ? 0.5 : 1);
        break;
      case 'F4':
        event.preventDefault();
        toggleLoop();
        break;
      default:
        break;
    }
  });

  await refreshLibrary();

  if (await invoke<boolean>('is_selftest')) {
    await runSelfTest(editor, (report) => invoke('save_diagnostics', { report }));
  }
}

void main();
