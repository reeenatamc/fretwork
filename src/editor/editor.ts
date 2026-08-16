/**
 * Editor de tablaturas: une el teclado, el estado de Rust y el renderizado de alphaTab.
 *
 * La partitura vive en Rust. Aquí sólo se manda la operación y se pinta lo que vuelve.
 *
 * El cursor no se dibuja dentro del SVG de alphaTab —sería pelearse con su maquetación—
 * sino en una rejilla del compás actual debajo de la partitura. Además de ser más simple,
 * se escribe mejor: se ve el compás en el que estás sin buscarlo en la página.
 */
import * as alphaTab from '@coderline/alphatab';
import {
  type Cursor,
  type CursorBounds,
  createCursor,
  moveBeat,
  moveString,
  moveToBar,
  toAddr,
} from './cursor';
import { FretAccumulator, type Intent, interpret, stepDuration } from './keymap';

interface SessionView {
  tex: string;
  title: string;
  bar_count: number;
  can_undo: boolean;
  can_redo: boolean;
}

interface NoteSummary {
  string: number;
  fret: number;
  techniques: number;
}

interface BeatSummary {
  addr: { bar: number; beat: number; voice: number };
  duration: number;
  dots: number;
  is_rest: boolean;
  notes: NoteSummary[];
}

interface BarView {
  beats: BeatSummary[];
  numerator: number;
  denominator: number;
  is_full: boolean;
}

/** Cuántas cuerdas tiene la guitarra. De momento fijo; saldrá de la afinación en M3. */
const STRING_COUNT = 6;

/** Nombres de las cuerdas al aire, de la 1ª a la 6ª. */
const STRING_NAMES = ['e', 'B', 'G', 'D', 'A', 'E'];

/**
 * Grosor de la línea de cada cuerda, siguiendo su calibre real.
 *
 * En el papel pautado todas las líneas son iguales, pero en una guitarra la sexta es
 * gruesa y la prima fina. Respetarlo hace que la rejilla se lea como un mástil en vez de
 * como una tabla, y ayuda a saber en qué cuerda estás sin contar.
 */
const STRING_WEIGHTS = ['1px', '1px', '1.5px', '2px', '2.5px', '3px'];

export class Editor {
  private view: SessionView | null = null;
  private cursor: Cursor = createCursor();
  private bar: BeatSummary[] = [];
  /** Pulsos que caben en el compás actual según su indicación. */
  private slots = 4;
  /** Si las figuras del compás actual ya suman el compás entero. */
  private barIsFull = false;
  /** Último error mostrado, para que la autocomprobación pueda leerlo. */
  lastError = '';
  private readonly frets = new FretAccumulator();
  private api: alphaTab.AlphaTabApi | null = null;
  private duration = 4;
  private dots = 0;
  private status = '';

  constructor(
    private readonly scoreHost: HTMLElement,
    private readonly gridHost: HTMLElement,
    private readonly statusHost: HTMLElement,
  ) {}

  /** Arranca el editor con una partitura nueva. */
  async start(title: string, barCount: number, tempo: number): Promise<void> {
    this.api = new alphaTab.AlphaTabApi(this.scoreHost, {
      core: { tex: true, fontDirectory: '/font/' },
      display: { scale: 0.9 },
      player: {
        playerMode: alphaTab.PlayerMode.EnabledSynthesizer,
        soundFont: '/soundfont/sonivox.sf3',
        scrollMode: alphaTab.ScrollMode.Off,
      },
    });

    this.view = await invoke<SessionView>('session_new', {
      title,
      barCount,
      tempoBpm: tempo,
    });

    await this.refresh();
    window.addEventListener('keydown', (event) => void this.onKey(event));
  }

  /**
   * Recarga desde Rust. Se llama al abrir una canción guardada.
   *
   * El cursor vuelve al principio: tras cambiar de canción, dejarlo donde estaba apuntaría
   * a un compás que ya no significa lo mismo.
   */
  async reload(): Promise<void> {
    this.view = await invoke<SessionView>('session_view', {});
    this.cursor = createCursor();
    this.frets.reset();
    // La figura vuelve a la negra: arrastrar la del trabajo anterior a una canción nueva
    // hace que se escriba con una figura que nadie eligió.
    this.duration = 4;
    this.dots = 0;
    await this.refresh();
  }

  /**
   * Muestra una versión propuesta sin cambiar la partitura de la sesión.
   *
   * Sirve para escuchar un arreglo antes de quedárselo: el criterio musical no se puede
   * dar por bueno sin oírlo.
   */
  showPreview(tex: string): void {
    this.api?.tex(tex);
  }

  /** Reproduce o pausa la partitura que se está mostrando. */
  play(): void {
    this.api?.playPause();
  }

  /** Devuelve el AlphaTex actual. Lo usa la autocomprobación. */
  currentTex(): string {
    return this.view?.tex ?? '';
  }

  /** Devuelve el cursor actual. Lo usa la autocomprobación. */
  currentCursor(): Cursor {
    return { ...this.cursor };
  }

  private bounds(): CursorBounds {
    return {
      barCount: this.view?.bar_count ?? 1,
      stringCount: STRING_COUNT,
      // Mientras el compás tenga sitio se deja escribir un pulso más; en cuanto las
      // figuras suman el compás entero, avanzar salta al siguiente.
      //
      // Es una regla musical, no de conteo: cuatro negras llenan un 4/4 igual que ocho
      // corcheas. Contar pulsos a secas hacía que el compás creciera sin fin y no se
      // pudiera avanzar escribiendo.
      beatsPerBar: () => Math.max(1, this.bar.length + (this.barIsFull ? 0 : 1)),
    };
  }

  /** Procesa una tecla. Público para que la autocomprobación pueda simular pulsaciones. */
  async onKey(event: KeyboardEvent): Promise<void> {
    const intent = interpret(event);
    if (!intent) return;
    event.preventDefault();
    await this.handle(intent);
  }

  private async handle(intent: Intent): Promise<void> {
    const addr = toAddr(this.cursor);

    switch (intent.type) {
      case 'moveString':
        this.frets.reset();
        this.cursor = moveString(this.cursor, intent.delta, this.bounds());
        break;

      case 'moveBeat':
        this.frets.reset();
        this.cursor = moveBeat(this.cursor, intent.delta, this.bounds());
        await this.loadBar();
        break;

      case 'moveBar':
        this.frets.reset();
        this.cursor = moveToBar(this.cursor, this.cursor.bar + intent.delta, this.bounds());
        await this.loadBar();
        break;

      case 'advance':
        this.frets.reset();
        this.cursor = moveBeat(this.cursor, 1, this.bounds());
        await this.loadBar();
        break;

      case 'digit': {
        const fret = this.frets.push(intent.value, Date.now());
        await this.send('session_apply_batch', {
          commands: [
            { kind: 'set_note', addr, string: this.cursor.string, fret },
            { kind: 'set_duration', addr, duration: this.duration, dots: this.dots },
          ],
        });
        this.status = `traste ${fret} en la ${this.cursor.string}ª cuerda`;
        break;
      }

      case 'clearString':
        await this.send('session_apply', {
          command: { kind: 'clear_string', addr, string: this.cursor.string },
        });
        break;

      case 'removeBeat':
        await this.send('session_apply', { command: { kind: 'remove_beat', addr } });
        break;

      case 'insertBeat':
        await this.send('session_apply', {
          command: { kind: 'insert_beat', addr, duration: this.duration },
        });
        break;

      case 'setRest':
        await this.send('session_apply', {
          command: { kind: 'set_rest', addr, is_rest: true },
        });
        break;

      case 'changeDuration':
        this.duration = stepDuration(this.duration, intent.direction);
        this.status = `figura 1/${this.duration}`;
        // Si el pulso ya existe, se le cambia la figura; si no, queda para la próxima nota.
        if (this.beatAtCursor()) {
          await this.send('session_apply', {
            command: { kind: 'set_duration', addr, duration: this.duration, dots: this.dots },
          });
        }
        break;

      case 'toggleDot':
        this.dots = this.dots === 0 ? 1 : 0;
        if (this.beatAtCursor()) {
          await this.send('session_apply', {
            command: { kind: 'set_duration', addr, duration: this.duration, dots: this.dots },
          });
        }
        break;

      case 'toggleTechnique': {
        const note = this.noteAtCursor();
        if (!note) {
          this.status = 'no hay nota en esta cuerda';
          break;
        }
        const on = (note.techniques & intent.bit) === 0;
        await this.send('session_apply', {
          command: {
            kind: 'set_technique',
            addr,
            string: this.cursor.string,
            technique: intent.bit,
            on,
          },
        });
        break;
      }

      case 'undo':
        await this.send('session_undo', {});
        break;

      case 'redo':
        await this.send('session_redo', {});
        break;

      case 'play':
        this.api?.playPause();
        break;
    }

    this.render();
  }

  /** Manda una operación a Rust y refresca. Los errores se muestran, no se tragan. */
  private async send(command: string, args: Record<string, unknown>): Promise<void> {
    try {
      this.view = await invoke<SessionView>(command, args);
      await this.refresh();
    } catch (error) {
      this.lastError = formatError(error);
      this.status = `⚠ ${this.lastError}`;
    }
  }

  private async refresh(): Promise<void> {
    if (this.view && this.api) {
      this.api.tex(this.view.tex);
    }
    await this.loadBar();
  }

  private async loadBar(): Promise<void> {
    try {
      const view = await invoke<BarView>('session_bar_notes', { bar: this.cursor.bar });
      this.bar = view.beats;
      this.slots = Math.max(1, view.numerator);
      this.barIsFull = view.is_full;
    } catch (error) {
      this.bar = [];
      this.lastError = formatError(error);
    }
    this.render();
  }

  private beatAtCursor(): BeatSummary | undefined {
    return this.bar[this.cursor.beat];
  }

  private noteAtCursor(): NoteSummary | undefined {
    return this.beatAtCursor()?.notes.find((note) => note.string === this.cursor.string);
  }

  /** Pinta la rejilla del compás actual y la barra de estado. */
  private render(): void {
    this.renderGrid();
    this.renderStatus();
  }

  private renderGrid(): void {
    // Se muestran siempre al menos los pulsos que caben en el compás, más uno para
    // escribir. Las columnas se reparten el ancho, así que nunca hay scroll horizontal.
    const columns = Math.max(this.bar.length + 1, this.cursor.beat + 1, this.slots);
    const rows: string[] = [];

    for (let string = 1; string <= STRING_COUNT; string += 1) {
      const cells: string[] = [];
      for (let index = 0; index < columns; index += 1) {
        const beat = this.bar[index];
        const note = beat?.notes.find((n) => n.string === string);
        const isCursor = index === this.cursor.beat && string === this.cursor.string;

        let text = '·';
        if (note) text = String(note.fret);
        else if (beat?.is_rest && string === 1) text = 'r';

        const classes = ['cell'];
        if (isCursor) classes.push('cursor');
        if (note) classes.push('has-note');
        // Marca de tiempo fuerte, para no perder la referencia rítmica al escribir.
        if (index > 0 && index % this.beatsPerPulse() === 0) classes.push('downbeat');
        cells.push(`<span class="${classes.join(' ')}">${text}</span>`);
      }

      rows.push(
        `<div class="grid-row" style="--cols:${columns};--string-weight:${STRING_WEIGHTS[string - 1]}">` +
          `<span class="string-name">${STRING_NAMES[string - 1]}</span>${cells.join('')}</div>`,
      );
    }

    this.gridHost.innerHTML = rows.join('');
  }

  /** Cada cuántas columnas cae un tiempo fuerte. */
  private beatsPerPulse(): number {
    return Math.max(1, Math.round(this.bar.length / this.slots)) || 1;
  }

  private renderStatus(): void {
    const dotted = this.dots > 0 ? ' con puntillo' : '';
    const parts = [
      `compás <b>${this.cursor.bar + 1}</b> de ${this.view?.bar_count ?? '?'}`,
      `cuerda <b>${this.cursor.string}ª</b>`,
      `figura <b>1/${this.duration}${dotted}</b>`,
      `<kbd>↑↓</kbd> cuerda <kbd>←→</kbd> pulso <kbd>0-9</kbd> traste <kbd>F4</kbd> bucle`,
    ];
    if (this.status) parts.push(this.status);

    this.statusHost.innerHTML = parts.join('<span class="sep">·</span>');
  }
}

/** Envoltorio de `invoke` que carga la API de Tauri sólo cuando hace falta. */
async function invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import('@tauri-apps/api/core');
  return tauriInvoke<T>(command, args);
}

/** Los errores de Rust llegan como objeto serializado; se muestran legibles. */
function formatError(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object') {
    const values = Object.values(error as Record<string, unknown>);
    if (values.length === 1 && typeof values[0] === 'string') return values[0];
    return JSON.stringify(error);
  }
  return String(error);
}
