import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useFilesStore } from './files'
import type { FileInfo } from '../types/FileInfo'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

function info(path: string): FileInfo {
  const image = path.endsWith('.png')
  return {
    path,
    name: path.split('/').pop() ?? path,
    extension: path.split('.').pop() ?? null,
    size: 1,
    category: image ? 'image' : 'audio',
    image: image ? { width: 1, height: 1, format: 'png' } : null,
    pdf: null,
    media: null,
  }
}

describe('files store', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    invoke.mockReset()
    invoke.mockImplementation(async (_cmd: string, args: { path: string }) => {
      if (args.path.includes('missing')) throw { code: 'notFound', message: args.path }
      return info(args.path)
    })
  })

  it('adds unique files in order and reports failures', async () => {
    const store = useFilesStore()
    const failures = await store.add(['/a.png', '/missing.png', '/a.png', '/b.mp3'])

    expect(store.files.map((f) => f.path)).toEqual(['/a.png', '/b.mp3'])
    expect(failures).toEqual([{ path: '/missing.png', error: { code: 'notFound', message: '/missing.png' } }])
    expect(invoke).toHaveBeenCalledTimes(3)
    expect(store.categories.get('image')).toBe(1)
  })

  it('moves files within their category only', async () => {
    const store = useFilesStore()
    await store.add(['/a.png', '/x.mp3', '/b.png', '/c.png'])

    store.move('/b.png', -1)
    expect(store.ofCategory('image').map((f) => f.path)).toEqual(['/b.png', '/a.png', '/c.png'])
    expect(store.files[1]?.path).toBe('/x.mp3')

    store.move('/b.png', -1)
    expect(store.ofCategory('image')[0]?.path).toBe('/b.png')
  })

  it('clears one category or everything', async () => {
    const store = useFilesStore()
    await store.add(['/a.png', '/x.mp3'])
    store.clear('image')
    expect(store.files.map((f) => f.path)).toEqual(['/x.mp3'])
    store.clear()
    expect(store.files).toEqual([])
  })
})
