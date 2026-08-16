/**
 * Envoltorio del IFrame Player API de YouTube.
 *
 * Este archivo es el que decide si el plan se sostiene: existe un issue abierto de Tauri
 * (tauri-apps/tauri#14422) donde el reproductor devuelve Error 153 en builds empaquetadas,
 * porque el protocolo custom no produce un origin que YouTube acepte. En Windows, Tauri v2
 * sirve desde http://tauri.localhost (un origin HTTP real), así que puede que no falle.
 *
 * Por eso `lastError` se expone: el arnés de M0 lo lee para diagnosticar.
 */

/** Códigos de error del IFrame API, con traducción al español. */
const YT_ERRORS: Record<number, string> = {
  2: 'Parámetro inválido (id de vídeo mal formado)',
  5: 'Error del reproductor HTML5',
  100: 'Vídeo no encontrado o privado',
  101: 'El dueño no permite reproducción embebida',
  150: 'El dueño no permite reproducción embebida',
  153: 'ERROR DE ORIGIN — el reproductor rechaza el origin de la app (el fallo que tememos)',
};

export interface YouTubePlayerEvents {
  onReady?: () => void;
  onError?: (code: number, message: string) => void;
  onStateChange?: (state: number) => void;
  /** Se dispara ~60 veces por segundo mientras reproduce. Alimenta el cursor de alphaTab. */
  onTime?: (seconds: number) => void;
}

export interface AbLoop {
  start: number;
  end: number;
}

declare global {
  interface Window {
    YT?: any;
    onYouTubeIframeAPIReady?: () => void;
  }
}

/**
 * Saca el identificador de un enlace de YouTube, o lo devuelve tal cual si ya lo es.
 *
 * Acepta las formas que uno acaba copiando de la barra del navegador: `watch?v=`,
 * `youtu.be/`, `/embed/` y `/shorts/`, con los parámetros de sobra que traen pegados.
 */
export function extractVideoId(input: string): string | null {
  const text = input.trim();
  if (!text) return null;

  // Un identificador suelto: once caracteres de letras, números, guion y guion bajo.
  if (/^[\w-]{11}$/.test(text)) return text;

  const patterns = [
    /[?&]v=([\w-]{11})/,
    /youtu\.be\/([\w-]{11})/,
    /\/embed\/([\w-]{11})/,
    /\/shorts\/([\w-]{11})/,
  ];

  for (const pattern of patterns) {
    const match = text.match(pattern);
    if (match?.[1]) return match[1];
  }

  return null;
}

let apiLoading: Promise<void> | null = null;

/** Carga el script del IFrame API una sola vez. */
function loadIframeApi(): Promise<void> {
  if (window.YT?.Player) return Promise.resolve();
  if (apiLoading) return apiLoading;

  apiLoading = new Promise<void>((resolve, reject) => {
    const timeout = window.setTimeout(
      () =>
        reject(new Error('El script del IFrame API no cargó en 15s (¿CSP bloqueando script-src?)')),
      15_000,
    );

    window.onYouTubeIframeAPIReady = () => {
      window.clearTimeout(timeout);
      resolve();
    };

    const script = document.createElement('script');
    script.src = 'https://www.youtube.com/iframe_api';
    script.async = true;
    script.onerror = () => {
      window.clearTimeout(timeout);
      reject(new Error('No se pudo descargar el script del IFrame API'));
    };
    document.head.appendChild(script);
  });

  return apiLoading;
}

export class YouTubePlayer {
  private player: any = null;
  private rafId: number | null = null;
  private loop: AbLoop | null = null;

  /** Último error recibido, para diagnóstico. `null` si nunca falló. */
  lastError: { code: number; message: string } | null = null;

  constructor(
    private readonly container: HTMLElement,
    private readonly events: YouTubePlayerEvents = {},
  ) {}

  async load(videoId: string): Promise<void> {
    await loadIframeApi();

    return new Promise<void>((resolve, reject) => {
      const timeout = window.setTimeout(
        () => reject(new Error('El reproductor no llegó a estar listo en 15s')),
        15_000,
      );

      this.player = new window.YT.Player(this.container, {
        videoId,
        playerVars: {
          enablejsapi: 1,
          // `origin` es justamente lo que Error 153 discute. Mandamos el real.
          origin: window.location.origin,
          controls: 1,
          rel: 0,
          modestbranding: 1,
        },
        events: {
          onReady: () => {
            window.clearTimeout(timeout);
            this.startTicking();
            this.events.onReady?.();
            resolve();
          },
          onError: (e: { data: number }) => {
            const message = YT_ERRORS[e.data] ?? `Error desconocido (${e.data})`;
            this.lastError = { code: e.data, message };
            window.clearTimeout(timeout);
            this.events.onError?.(e.data, message);
            reject(new Error(`YouTube Error ${e.data}: ${message}`));
          },
          onStateChange: (e: { data: number }) => this.events.onStateChange?.(e.data),
        },
      });
    });
  }

  /** Bucle de tiempo: alimenta onTime y hace cumplir el loop A-B. */
  private startTicking(): void {
    const tick = () => {
      if (this.player?.getCurrentTime) {
        const t = this.player.getCurrentTime() as number;
        // YouTube no tiene loop A-B nativo; lo implementamos nosotros.
        if (this.loop && t >= this.loop.end) {
          this.seekTo(this.loop.start);
        } else {
          this.events.onTime?.(t);
        }
      }
      this.rafId = requestAnimationFrame(tick);
    };
    this.rafId = requestAnimationFrame(tick);
  }

  play(): void {
    this.player?.playVideo();
  }

  pause(): void {
    this.player?.pauseVideo();
  }

  /** Alterna entre reproducir y pausar. Estado 1 es "reproduciendo". */
  toggle(): void {
    if (this.player?.getPlayerState?.() === 1) {
      this.pause();
    } else {
      this.play();
    }
  }

  seekTo(seconds: number): void {
    this.player?.seekTo(Math.max(0, seconds), true);
  }

  /** Retrocede N segundos. El atajo más usado al transcribir. */
  rewind(seconds = 3): void {
    this.seekTo(this.getCurrentTime() - seconds);
  }

  getCurrentTime(): number {
    return (this.player?.getCurrentTime?.() as number) ?? 0;
  }

  getDuration(): number {
    return (this.player?.getDuration?.() as number) ?? 0;
  }

  /** Velocidades típicas: [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2]. */
  setPlaybackRate(rate: number): void {
    this.player?.setPlaybackRate(rate);
  }

  getPlaybackRate(): number {
    return (this.player?.getPlaybackRate?.() as number) ?? 1;
  }

  getAvailablePlaybackRates(): number[] {
    return (this.player?.getAvailablePlaybackRates?.() as number[]) ?? [];
  }

  setLoop(loop: AbLoop | null): void {
    this.loop = loop;
    if (loop) this.seekTo(loop.start);
  }

  getLoop(): AbLoop | null {
    return this.loop;
  }

  destroy(): void {
    if (this.rafId !== null) cancelAnimationFrame(this.rafId);
    this.rafId = null;
    this.player?.destroy?.();
    this.player = null;
  }
}
