import { defineConfig, loadEnv } from 'vite'

const matrixJSX = {
  runtime: 'automatic',
  importSource: '@mickyballadelli/matrix'
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const target = env.LINKS_HTTP_TARGET || 'http://127.0.0.1:8080'

  return {
    oxc: {
      jsx: matrixJSX
    },
    optimizeDeps: {
      rolldownOptions: {
        transform: {
          jsx: matrixJSX
        }
      }
    },
    resolve: {
      preserveSymlinks: true
    },
    server: {
      port: 5174,
      strictPort: true,
      proxy: {
        '/links-api': {
          target,
          changeOrigin: true,
          rewrite: path => path.replace(/^\/links-api/, '')
        }
      }
    },
    preview: {
      port: 4174,
      strictPort: true
    }
  }
})
