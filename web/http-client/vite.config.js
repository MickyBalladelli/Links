import { defineConfig, loadEnv } from 'vite'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const target = env.LINKS_HTTP_TARGET || 'http://127.0.0.1:8080'

  return {
    oxc: {
      jsx: {
        runtime: 'automatic',
        importSource: '@mickyballadelli/matrix'
      }
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
