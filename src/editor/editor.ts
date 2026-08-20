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
  moveToBeat,
  toAddr,
} from './cursor';
import { fretboardHtml } from './fretboard';
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
  filled: number;
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

/**
 * Aviso mientras el banco de sonidos no está listo.
 *
 * El sintetizador tarda un momento en arrancar y, hasta entonces, pulsar Intro no hacía
 * nada: parecía que el audio estuviera roto en vez de cargándose.
 */
const SOUND_LOADING = 'el sonido aún se está cargando';

/**
 * Volumen del metrónomo.
 *
 * A tope tapa a la guitarra, que es justo lo que se está intentando oír.
 */
const METRONOME_VOLUME = 0.7;

/**
 * Lo que el editor necesita de la página.
 *
 * Se recibe como un objeto y no como cinco parámetros sueltos porque ya iban seis y el
 * orden empezaba a ser una trampa: dos `HTMLElement` seguidos se intercambian sin que el
 * compilador diga nada.
 */
export interface EditorHosts {
  /** Donde alphaTab dibuja la partitura. */
  scoreHost: HTMLElement;
  /** Donde va la rejilla del compás actual. */
  gridHost: HTMLElement;
  /** La barra de estado. */
  statusHost: HTMLElement;
  /** El mástil. */
  fretboardHost: HTMLElement;
  /**
   * Si el teclado es de otro: un panel abierto, por ejemplo.
   *
   * Se recibe como función para que el editor no tenga que saber qué paneles existen.
   */
  isSuspended?: () => boolean;
  /**
   * Marca o desmarca el compás como atragantado.
   *
   * El editor sabe en qué compás está el cursor, pero el progreso no es cosa suya: lo
   * avisa y quien lleva el progreso decide.
   */
  onToggleTricky?: (bar: number) => void;
}

export class Editor {
  private view: SessionView | null = null;
  private cursor: Cursor = createCursor();
  private bar: BeatSummary[] = [];
  /** Pulsos que caben en el compás actual según su indicación. */
  private slots = 4;
  /** Si las figuras del compás actual ya suman el compás entero. */
  private barIsFull = false;
  /** Qué parte del compás actual está escrita, de 0 a 1 o más. */
  private barFilled = 0;
  /** Último error mostrado, para que la autocomprobación pueda leerlo. */
  lastError = '';
  private readonly frets = new FretAccumulator();
  private api: alphaTab.AlphaTabApi | null = null;
  private duration = 4;
  private dots = 0;
  private status = '';
  /** Si la partitura está sonando ahora mismo. */
  private playing = false;
  /** Si lo que se muestra es un arreglo propuesto y no la partitura de la sesión. */
  private previewing = false;
  /** Dónde estaba el cursor al dar al play, para volver ahí al terminar. */
  private resumeCursor: Cursor | null = null;
  /** Compás que se está repitiendo en bucle, o `null` si no hay bucle. */
  private loopBar: number | null = null;
  /** Si el metrónomo está sonando. */
  private metronome = false;
  /** Compases marcados como atragantados en esta canción. */
  private trickyBars: readonly number[] = [];

  constructor(private readonly ui: EditorHosts) {}

  /** Arranca el editor con una partitura nueva. */
  async start(title: string, barCount: number, tempo: number): Promise<void> {
    this.api = new alphaTab.AlphaTabApi(this.ui.scoreHost, {
      core: { tex: true, fontDirectory: '/font/' },
      display: { scale: 0.9 },
      player: {
        playerMode: alphaTab.PlayerMode.EnabledSynthesizer,
        soundFont: '/soundfont/sonivox.sf3',
        // Durante la reproducción sí interesa el cursor de alphaTab: marca el compás y el
        // pulso dentro de la partitura grabada, que es donde se mira al repasar.
        enableCursor: true,
        enableAnimatedBeatCursor: true,
        // El scroll ocurre dentro del panel de la partitura, nunca arrastrando la ventana.
        scrollMode: alphaTab.ScrollMode.Continuous,
        scrollElement: this.ui.scoreHost,
        scrollOffsetY: -20,
      },
    });

    this.view = await invoke<SessionView>('session_new', {
      title,
      barCount,
      tempoBpm: tempo,
    });

    this.api.playedBeatChanged.on((beat) => this.followPlayback(beat));
    this.api.playerStateChanged.on((args) => this.onPlayerState(args));
    // Cada edición vuelve a cargar la partitura y con ella los pulsos MIDI cambian de
    // sitio; el bucle se recoloca para seguir cubriendo el compás que se está repitiendo.
    this.api.scoreLoaded.on(() => this.applyLoop());
    this.api.playerReady.on(() => {
      // Sólo se dice algo si alguien intentó sonar antes de tiempo. Si no, sobra.
      if (this.status !== SOUND_LOADING) return;
      this.status = 'sonido listo';
      this.renderStatus();
    });

    this.ui.fretboardHost.addEventListener('click', (event) => void this.onFretClick(event));

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
    this.previewing = true;
    this.api?.tex(tex);
  }

  /**
   * Suena desde el compás en el que está el cursor.
   *
   * Empezar siempre por el principio no sirve al transcribir: quien está sacando el
   * compás treinta quiere oír el treinta, no los veintinueve de antes. Al pausar y volver
   * a dar, el compás se repite entero desde su principio, que es lo que se quiere para
   * comprobar lo que se acaba de escribir.
   */
  play(): void {
    if (!this.api || !this.soundIsReady()) return;

    if (this.playing) {
      this.api.playPause();
      return;
    }

    // La propuesta de arreglo se escucha entera: es otra versión de la canción y sus
    // compases no tienen por qué caer donde los de la sesión.
    if (!this.previewing) {
      // Dar al play sale del bucle: para repetir un compás está Mayúsculas+Intro.
      this.setLoopBar(null);
      const ticks = this.barTicks(this.cursor.bar);
      if (ticks) this.api.tickPosition = ticks.startTick;
    }

    this.api.play();
  }

  /**
   * Repite sin parar el compás en el que está el cursor.
   *
   * Es el equivalente al bucle A–B del vídeo, pero del lado de la partitura: se compara
   * el compás recién escrito con la grabación tantas veces como haga falta sin tocar nada.
   */
  toggleBarLoop(): void {
    if (!this.api || !this.soundIsReady()) return;

    if (this.loopBar !== null) {
      this.setLoopBar(null);
      this.api.stop();
      this.status = 'bucle quitado';
      return;
    }

    const ticks = this.barTicks(this.cursor.bar);
    if (!ticks) {
      this.status = 'ese compás todavía no está en la partitura';
      return;
    }

    this.setLoopBar(this.cursor.bar);
    this.api.tickPosition = ticks.startTick;
    this.api.play();
    this.status = `bucle en el compás ${this.cursor.bar + 1}`;
  }

  /** Enciende o apaga el metrónomo. Ayuda a colocar el ritmo de lo que se escribe. */
  toggleMetronome(): void {
    if (!this.api) return;
    this.metronome = !this.metronome;
    this.api.metronomeVolume = this.metronome ? METRONOME_VOLUME : 0;
    this.status = this.metronome ? 'metrónomo encendido' : 'metrónomo apagado';
  }

  /** Guarda qué compás se repite y se lo pasa al reproductor. */
  private setLoopBar(bar: number | null): void {
    this.loopBar = bar;
    this.applyLoop();
  }

  /** Traslada el bucle al reproductor, recalculando los pulsos del compás. */
  private applyLoop(): void {
    if (!this.api) return;
    const ticks = this.loopBar === null ? null : this.barTicks(this.loopBar);
    this.api.playbackRange = ticks;
    this.api.isLooping = ticks !== null;
  }

  /**
   * Principio y final del compás en pulsos MIDI.
   *
   * Devuelve `null` si ese compás todavía no existe en la partitura renderizada, que pasa
   * mientras se escribe más allá del final.
   */
  private barTicks(bar: number): { startTick: number; endTick: number } | null {
    const masterBar = this.api?.score?.masterBars[bar];
    if (!masterBar) return null;
    return {
      startTick: masterBar.start,
      endTick: masterBar.start + masterBar.calculateDuration(),
    };
  }

  /** Avisa de que el sintetizador aún no está listo, en vez de no hacer nada. */
  private soundIsReady(): boolean {
    if (this.api?.isReadyForPlayback) return true;
    this.status = SOUND_LOADING;
    this.renderStatus();
    return false;
  }

  /**
   * Sustituye el banco de sonidos por uno cargado desde disco.
   *
   * El que trae alphaTab es pequeño a propósito y sus muestras de guitarra suenan
   * delgadas; con un banco dedicado la diferencia se oye de inmediato.
   */
  loadSoundFont(data: Uint8Array): void {
    this.api?.loadSoundFont(data, false);
  }

  /**
   * Recibe los compases que se atragantan en esta canción.
   *
   * El editor no los guarda ni decide cuáles son: sólo los enseña, para que al pasar por
   * uno se vea sin tener que abrir el repertorio.
   */
  showTrickyBars(bars: readonly number[]): void {
    this.trickyBars = bars;
    this.render();
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
    if (this.ui.isSuspended?.()) return;

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
        await this.writeFret(this.cursor.string, fret);
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
        this.play();
        break;

      case 'loopBar':
        this.toggleBarLoop();
        break;

      case 'toggleMetronome':
        this.toggleMetronome();
        break;

      case 'toggleTricky':
        this.ui.onToggleTricky?.(this.cursor.bar);
        break;
    }

    this.render();
  }

  /**
   * Pone un traste en una cuerda del pulso actual.
   *
   * La figura va en la misma operación que la nota: si se mandaran por separado, deshacer
   * dejaría la nota escrita con la duración de antes.
   */
  private async writeFret(string: number, fret: number): Promise<void> {
    const addr = toAddr(this.cursor);
    await this.send('session_apply_batch', {
      commands: [
        { kind: 'set_note', addr, string, fret },
        { kind: 'set_duration', addr, duration: this.duration, dots: this.dots },
      ],
    });
    this.status = `traste ${fret} en la ${string}ª cuerda`;
  }

  /**
   * Escribe el traste que se ha pulsado con el ratón en el mástil.
   *
   * Volver a pulsar la misma nota la quita: buscando una posición con los dedos se falla
   * y se corrige, y tener que ir al teclado a borrar rompe justo eso.
   */
  private async onFretClick(event: MouseEvent): Promise<void> {
    const target = (event.target as HTMLElement | null)?.closest<HTMLElement>('[data-fret]');
    if (!target || this.ui.isSuspended?.()) return;

    const string = Number(target.dataset.string);
    const fret = Number(target.dataset.fret);
    if (!Number.isInteger(string) || !Number.isInteger(fret)) return;

    // El foco vuelve al documento: después de un clic hay que poder seguir escribiendo sin
    // tener que pinchar en ningún sitio.
    target.blur();

    // El acumulador de dos cifras se corta: el clic ya dijo qué traste era.
    this.frets.reset();
    this.cursor = { ...this.cursor, string };

    const current = this.noteAtCursor();
    if (current?.fret === fret) {
      await this.send('session_apply', {
        command: { kind: 'clear_string', addr: toAddr(this.cursor), string },
      });
      this.status = `quitada la nota de la ${string}ª cuerda`;
    } else {
      await this.writeFret(string, fret);
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
      this.previewing = false;
      this.api.tex(this.view.tex);
    }
    await this.loadBar();
  }

  private async loadBar(): Promise<void> {
    // Al seguir la reproducción se piden compases más deprisa de lo que Rust contesta.
    // Si mientras tanto el cursor ya se fue a otro compás, la respuesta vieja se tira:
    // pintarla dejaría la rejilla mostrando un compás que ya no es el que suena.
    const requested = this.cursor.bar;
    try {
      const view = await invoke<BarView>('session_bar_notes', { bar: requested });
      if (this.cursor.bar !== requested) return;
      this.bar = view.beats;
      this.slots = Math.max(1, view.numerator);
      this.barIsFull = view.is_full;
      this.barFilled = view.filled;
    } catch (error) {
      this.bar = [];
      this.lastError = formatError(error);
    }
    this.render();
  }

  /**
   * Lleva el cursor al pulso que está sonando.
   *
   * La rejilla es donde se mira al escribir, así que al escuchar tiene que enseñar el
   * compás que suena y no el que se estaba editando. Que el cursor de edición sea el
   * mismo que sigue a la música tiene una ventaja al transcribir: se para el reproductor
   * donde falla la transcripción y ya se está escribiendo justo ahí.
   */
  private followPlayback(beat: alphaTab.model.Beat): void {
    // Mientras se escucha un arreglo propuesto, la partitura que suena no es la de la
    // sesión: seguirla movería el cursor a compases que no se corresponden con la rejilla.
    if (this.previewing) return;

    const bar = beat.voice?.bar?.index ?? 0;
    const changedBar = bar !== this.cursor.bar;

    // Con el cursor moviéndose solo, los dos dígitos seguidos de un traste dejan de tener
    // sentido: el segundo caería en otro pulso.
    this.frets.reset();
    this.cursor = moveToBeat(this.cursor, bar, beat.index, this.bounds());

    if (changedBar) void this.loadBar();
    else this.render();
  }

  /**
   * Arranque y parada del reproductor.
   *
   * Al parar del todo —final de la canción o stop— el cursor vuelve a donde se estaba
   * escribiendo. En pausa se queda donde sonaba, que es lo que se quiere cuando se para
   * para corregir un pasaje.
   */
  private onPlayerState(args: alphaTab.synth.PlayerStateChangedEventArgs): void {
    const playing = args.state === alphaTab.synth.PlayerState.Playing;

    if (playing && !this.playing) {
      this.resumeCursor = { ...this.cursor };
    } else if (!playing && args.stopped && this.resumeCursor) {
      this.cursor = this.resumeCursor;
      this.resumeCursor = null;
      void this.loadBar();
    }

    this.playing = playing;
    this.render();
  }

  private beatAtCursor(): BeatSummary | undefined {
    return this.bar[this.cursor.beat];
  }

  private noteAtCursor(): NoteSummary | undefined {
    return this.beatAtCursor()?.notes.find((note) => note.string === this.cursor.string);
  }

  /** Pinta la rejilla del compás actual, el mástil y la barra de estado. */
  private render(): void {
    this.renderGrid();
    this.renderFretboard();
    this.renderStatus();
  }

  /** Dibuja el mástil con las notas del pulso en el que está el cursor. */
  private renderFretboard(): void {
    const pressed = new Map<number, number>();
    for (const note of this.beatAtCursor()?.notes ?? []) {
      pressed.set(note.string, note.fret);
    }

    this.ui.fretboardHost.innerHTML = fretboardHtml({
      stringCount: STRING_COUNT,
      stringNames: STRING_NAMES,
      cursorString: this.cursor.string,
      pressed,
    });
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
        // Sonando, el cursor cambia de color: deja claro que se está moviendo solo y que
        // lo que se teclee caerá donde vaya la música.
        if (isCursor && this.playing) classes.push('playing');
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

    this.ui.gridHost.innerHTML = rows.join('');
  }

  /**
   * Estado de llenado del compás actual.
   *
   * Dejar un compás a medias sin darse cuenta es fácil al transcribir, y entonces lo que
   * se escribe después acaba metido dentro del compás incompleto. Verlo mientras se
   * escribe evita el enredo en lugar de tener que deshacerlo.
   */
  private fillLabel(): string {
    if (this.bar.length === 0) return '<span class="fill-empty">compás vacío</span>';
    if (this.barFilled > 1.001) {
      return `<span class="fill-over">se pasa ${Math.round(this.barFilled * 100)} %</span>`;
    }
    if (this.barIsFull) return '<span class="fill-ok">compás completo</span>';
    return `<span class="fill-partial">falta ${Math.round((1 - this.barFilled) * 100)} %</span>`;
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
      this.fillLabel(),
      `<kbd>↑↓</kbd> cuerda <kbd>←→</kbd> pulso <kbd>0-9</kbd> traste <kbd>Intro</kbd> sonar`,
    ];
    if (this.playing) parts.push('<span class="fill-ok">▶ sonando</span>');
    if (this.loopBar !== null) parts.push(`⟲ compás <b>${this.loopBar + 1}</b>`);
    if (this.metronome) parts.push('♩ metrónomo');
    if (this.trickyBars.includes(this.cursor.bar)) {
      parts.push('<span class="fill-over">✱ se atraganta</span>');
    }
    if (this.status) parts.push(this.status);

    this.ui.statusHost.innerHTML = parts.join('<span class="sep">·</span>');
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
