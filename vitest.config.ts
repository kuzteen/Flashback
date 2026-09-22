import { defineConfig } from 'vitest/config';

// Configuración aparte de vite.config.ts: los tests cubren módulos TS puros y no necesitan el
// plugin de SvelteKit, que además arrancaría el servidor de desarrollo de Tauri.
export default defineConfig({
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
