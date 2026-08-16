/**
 * Impresión de partituras dentro del webview.
 *
 * `AlphaTabApi.print()` no sirve aquí: abre una ventana emergente con `window.open`,
 * que en el webview de Tauri devuelve `null`. Y `Ctrl+P` a secas tampoco vale, porque
 * alphaTab virtualiza el viewport y sólo renderiza lo visible.
 *
 * La solución es renderizar la partitura entera en un contenedor aparte con la carga
 * diferida desactivada, y dejar que la hoja de estilos de impresión oculte todo lo demás.
 */
import * as alphaTab from '@coderline/alphatab';

/** Identificador del contenedor de impresión, referenciado desde `styles.css`. */
const PRINT_HOST_ID = 'print-host';

/** Tiempo máximo de espera al renderizado completo antes de rendirse. */
const RENDER_TIMEOUT_MS = 30_000;

export interface PrintOptions {
  /** Escala de la partitura. 0.8 entra bien en A4. */
  scale?: number;
  /** Cuánto estirar los compases para llenar el ancho. */
  stretchForce?: number;
  /**
   * Prepara todo pero no abre el diálogo del sistema.
   *
   * Sirve para verificar en automático la parte que de verdad puede fallar —el
   * renderizado completo en el contenedor oculto— sin que el diálogo nativo
   * bloquee el webview.
   */
  dryRun?: boolean;
}

/** Resultado de una preparación de impresión, para diagnóstico. */
export interface PrintResult {
  /** Páginas A4 que ocupó la partitura. */
  pages: number;
  /** Milisegundos que tardó el renderizado. */
  renderMs: number;
}

/**
 * Imprime una partitura escrita en AlphaTex.
 *
 * Renderiza en un contenedor oculto, espera a que termine, lanza el diálogo de
 * impresión del sistema y limpia después.
 */
export async function printTex(tex: string, options: PrintOptions = {}): Promise<PrintResult> {
  const { scale = 0.8, stretchForce = 0.8, dryRun = false } = options;
  const startedAt = Date.now();

  const host = document.createElement('div');
  host.id = PRINT_HOST_ID;
  document.body.appendChild(host);

  let api: alphaTab.AlphaTabApi | null = null;

  try {
    api = new alphaTab.AlphaTabApi(host, {
      core: {
        tex: true,
        fontDirectory: '/font/',
        // Clave: sin esto sólo se imprimiría la parte visible en pantalla.
        enableLazyLoading: false,
      },
      display: {
        scale,
        stretchForce,
        layoutMode: alphaTab.LayoutMode.Page,
      },
      player: {
        // El reproductor no pinta nada al imprimir y cuesta arrancarlo.
        playerMode: alphaTab.PlayerMode.Disabled,
      },
    });

    await renderAndWait(api, tex);

    // El navegador necesita un fotograma para aplicar los estilos de impresión.
    await nextFrame();

    const renderMs = Date.now() - startedAt;
    // Una hoja A4 con márgenes de 12mm deja unos 273mm útiles de alto.
    const pages = Math.max(1, Math.ceil(host.getBoundingClientRect().height / (273 * 3.7795)));

    if (host.querySelectorAll('svg').length === 0) {
      throw new Error('el contenedor de impresión quedó sin partitura renderizada');
    }

    if (!dryRun) {
      window.print();
    }

    return { pages, renderMs };
  } finally {
    api?.destroy();
    host.remove();
  }
}

/** Carga el AlphaTex y resuelve cuando el renderizado terminó. */
function renderAndWait(api: alphaTab.AlphaTabApi, tex: string): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const timeout = window.setTimeout(
      () => reject(new Error('El renderizado para impresión superó los 30s')),
      RENDER_TIMEOUT_MS,
    );

    api.renderFinished.on(() => {
      window.clearTimeout(timeout);
      resolve();
    });

    api.error.on((e) => {
      window.clearTimeout(timeout);
      reject(new Error(`alphaTab falló al renderizar para impresión: ${e}`));
    });

    api.tex(tex);
  });
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}
