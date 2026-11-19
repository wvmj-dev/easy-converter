import { onMounted, onUnmounted, ref } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import type { UnlistenFn } from '@tauri-apps/api/event'

/**
 * Native drag and drop: Tauri delivers the dropped files' paths, never their contents.
 */
export function useDragDrop(onDrop: (paths: string[]) => void) {
  const isOver = ref(false)
  let unlisten: UnlistenFn | null = null
  let disposed = false

  onMounted(async () => {
    const stop = await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload
      if (payload.type === 'enter' || payload.type === 'over') {
        isOver.value = true
      } else if (payload.type === 'leave') {
        isOver.value = false
      } else if (payload.type === 'drop') {
        isOver.value = false
        if (payload.paths.length > 0) {
          onDrop(payload.paths)
        }
      }
    })
    if (disposed) {
      stop()
    } else {
      unlisten = stop
    }
  })

  onUnmounted(() => {
    disposed = true
    unlisten?.()
  })

  return { isOver }
}
