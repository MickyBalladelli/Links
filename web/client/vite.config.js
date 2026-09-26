import { defineConfig, loadEnv } from 'vite'

const matrixJSX = {
  runtime: 'automatic',
  importSource: '@mickyballadelli/matrix'
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const authTarget = env.LINKS_AUTH_TARGET || 'http://127.0.0.1:8080'
  const gatewayTarget = env.LINKS_GATEWAY_TARGET || 'http://127.0.0.1:8081'

  return {
    oxc: { jsx: matrixJSX },
    optimizeDeps: {
      rolldownOptions: {
        transform: { jsx: matrixJSX }
      }
    },
    resolve: { preserveSymlinks: true },
    server: {
      port: 5175,
      strictPort: true,
      proxy: {
        '/links-api': {
          target: authTarget,
          changeOrigin: true,
          rewrite: path => path.replace(/^\/links-api/, '')
        },
        '/v1': {
          target: gatewayTarget,
          changeOrigin: true,
          ws: true
        }
      }
    },
    preview: { port: 4175, strictPort: true }
  }
})
