import { describe, expect, it } from 'vitest'
import {
  IMAGE_OPERATIONS,
  buildImageRequests,
  defaultImageOptions,
  unavailableReason,
  validateOptions,
} from './image'
import type { FileInfo } from '../types/FileInfo'
import type { ImageOutputFormat } from '../types/ImageOutputFormat'
import ptBR from '../i18n/pt-BR.json'
import en from '../i18n/en.json'

function file(name: string, width = 100, height = 50, format: ImageOutputFormat = 'png'): FileInfo {
  return {
    path: `/home/u/${name}`,
    name,
    extension: name.split('.').pop() ?? null,
    size: 1000,
    category: 'image',
    image: { width, height, format },
    pdf: null,
    media: null,
  }
}

describe('image operations', () => {
  it('builds one request per file, and one for merge', () => {
    const files = [file('a.png'), file('b.png')]
    const options = defaultImageOptions()

    expect(buildImageRequests('rotate', files, options)).toEqual([
      { type: 'imageRotate', input: '/home/u/a.png', rotation: 'cw90' },
      { type: 'imageRotate', input: '/home/u/b.png', rotation: 'cw90' },
    ])
    expect(buildImageRequests('merge', files, options)).toEqual([
      {
        type: 'imageMerge',
        inputs: ['/home/u/a.png', '/home/u/b.png'],
        direction: 'horizontal',
        format: 'png',
        matchSize: true,
        spacing: 0,
      },
    ])
  })

  it('sends quality only for JPEG conversions', () => {
    const options = defaultImageOptions()
    options.convert = { format: 'png', quality: 50 }
    expect(buildImageRequests('convert', [file('a.png')], options)[0]).toEqual({
      type: 'imageConvert',
      input: '/home/u/a.png',
      format: 'png',
    })
    options.convert.format = 'jpeg'
    expect(buildImageRequests('convert', [file('a.png')], options)[0]).toMatchObject({ quality: 50 })
  })

  it('copies resize options so later edits do not change sent requests', () => {
    const options = defaultImageOptions()
    const [request] = buildImageRequests('resize', [file('a.png')], options)
    if (options.resize.mode !== 'percent') options.resize.width = 1
    expect(request).toMatchObject({ resize: { mode: 'fit', width: 1920 } })
  })

  it('knows when an operation cannot be used', () => {
    expect(unavailableReason('merge', [file('a.png')])?.key).toBe('image.unavailable.needsTwo')
    expect(unavailableReason('merge', [file('a.png'), file('b.png')])).toBeNull()
    expect(unavailableReason('stripMetadata', [file('a.gif', 1, 1, 'gif')])?.key).toBe(
      'image.unavailable.unsupportedFormat',
    )
    expect(unavailableReason('stripMetadata', [file('a.jpg', 1, 1, 'jpeg')])).toBeNull()
    expect(unavailableReason('convert', [])?.key).toBe('image.unavailable.noFiles')
  })

  it('validates options like the backend', () => {
    const options = defaultImageOptions()
    const files = [file('small.png', 100, 50)]

    options.convert.quality = 0
    expect(validateOptions('convert', options, files)?.key).toBe('image.invalid.quality')
    options.convert.quality = 80.5
    expect(validateOptions('convert', options, files)?.key).toBe('image.invalid.quality')

    options.resize = { mode: 'percent', percent: 1001 }
    expect(validateOptions('resize', options, files)?.key).toBe('image.invalid.percent')
    options.resize = { mode: 'exact', width: 10, height: 10 }
    expect(validateOptions('resize', options, files)).toBeNull()

    options.crop = { x: 50, y: 0, width: 60, height: 10 }
    expect(validateOptions('crop', options, files)).toMatchObject({
      key: 'image.invalid.cropOutside',
      params: { name: 'small.png', width: 100, height: 50 },
    })
    options.crop = { x: 40, y: 0, width: 60, height: 50 }
    expect(validateOptions('crop', options, files)).toBeNull()

    options.merge.spacing = -1
    expect(validateOptions('merge', options, files)?.key).toBe('image.invalid.spacing')
  })

  it('has texts for every operation in every language', () => {
    for (const catalog of [ptBR, en]) {
      for (const op of IMAGE_OPERATIONS) {
        expect(catalog.image.ops[op].label).toBeTruthy()
      }
    }
  })
})
