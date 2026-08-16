import { defineConfig } from 'vite';
import { alphaTab } from '@coderline/alphatab-vite';

// El plugin de alphaTab cablea los Web Workers (render) y los AudioWorklets (sintetizador),
// y copia Bravura + sonivox.sf3 a /font y /soundfont en la salida.
export default defineConfig({
  plugins: [alphaTab()],

  // Tauri controla la consola; no la limpiemos encima de sus mensajes.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // src-tauri lo vigila cargo, no vite.
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    // WebView2 en Windows va sobrado con esto.
    target: 'chrome105',
    sourcemap: true,
  },
});
