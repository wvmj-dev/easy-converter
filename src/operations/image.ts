// Image operations offered by the interface: default options, availability, a light
// validation for immediate feedback and the requests sent to the backend. The backend
// validates everything again; the limits below only mirror `ops/image.rs`.
import type { FileInfo } from '../types/FileInfo'
import type { FlipAxis } from '../types/FlipAxis'
import type { ImageOutputFormat } from '../types/ImageOutputFormat'
import type { JobRequest } from '../types/JobRequest'
import type { MergeDirection } from '../types/MergeDirection'
import type { PdfPageSize } from '../types/PdfPageSize'
import type { ResizeMode } from '../types/ResizeMode'
import type { Rotation } from '../types/Rotation'

export const IMAGE_LIMITS = {
  maxDimension: 65_535,
  maxPercent: 1_000,
  maxMergeInputs: 50,
  maxSpacing: 2_000,
  maxPdfInputs: 200,
  maxPdfMarginMm: 50,
} as const

export const IMAGE_OPERATIONS = [
  'convert',
  'resize',
  'crop',
  'rotate',
  'flip',
  'compress',
  'stripMetadata',
  'merge',
  'toPdf',
] as const
export type ImageOperation = (typeof IMAGE_OPERATIONS)[number]

export const OUTPUT_FORMATS: ImageOutputFormat[] = ['jpeg', 'png', 'webp', 'gif', 'bmp', 'tiff', 'ico']

/** Formats whose metadata the backend can remove. */
export const STRIP_METADATA_FORMATS: ImageOutputFormat[] = ['jpeg', 'png', 'webp', 'tiff']

export interface CropArea {
  x: number
  y: number
  width: number
  height: number
}

export interface ImageOptions {
  convert: { format: ImageOutputFormat; quality: number }
  resize: ResizeMode
  crop: CropArea
  rotate: { rotation: Rotation }
  flip: { axis: FlipAxis }
  compress: { quality: number }
  merge: { direction: MergeDirection; format: ImageOutputFormat; matchSize: boolean; spacing: number }
  toPdf: { pageSize: PdfPageSize; marginMm: number }
}

/** Operations that make one file from all the images, in list order. */
export const COMBINING_OPERATIONS: ImageOperation[] = ['merge', 'toPdf']

export function defaultImageOptions(): ImageOptions {
  return {
    convert: { format: 'jpeg', quality: 90 },
    resize: { mode: 'fit', width: 1920, height: 1080 },
    crop: { x: 0, y: 0, width: 100, height: 100 },
    rotate: { rotation: 'cw90' },
    flip: { axis: 'horizontal' },
    compress: { quality: 75 },
    merge: { direction: 'horizontal', format: 'png', matchSize: true, spacing: 0 },
    toPdf: { pageSize: 'a4', marginMm: 10 },
  }
}

/** A problem to show to the user: an i18n key and its parameters. */
export interface Problem {
  key: string
  params?: Record<string, string | number>
}

function isIntIn(value: unknown, min: number, max: number): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max
}

/** Why an operation cannot be used with these files, or `null` when it can. */
export function unavailableReason(operation: ImageOperation, files: FileInfo[]): Problem | null {
  if (files.length === 0) {
    return { key: 'image.unavailable.noFiles' }
  }
  if (operation === 'merge') {
    if (files.length < 2) return { key: 'image.unavailable.needsTwo' }
    if (files.length > IMAGE_LIMITS.maxMergeInputs) {
      return { key: 'image.unavailable.tooMany', params: { max: IMAGE_LIMITS.maxMergeInputs } }
    }
  }
  if (operation === 'toPdf' && files.length > IMAGE_LIMITS.maxPdfInputs) {
    return { key: 'image.unavailable.tooMany', params: { max: IMAGE_LIMITS.maxPdfInputs } }
  }
  if (operation === 'stripMetadata') {
    const unsupported = files.find((f) => !f.image || !STRIP_METADATA_FORMATS.includes(f.image.format))
    if (unsupported) {
      return { key: 'image.unavailable.unsupportedFormat', params: { name: unsupported.name } }
    }
  }
  return null
}

/** First problem with the options, or `null` when they look valid. */
export function validateOptions(
  operation: ImageOperation,
  options: ImageOptions,
  files: FileInfo[],
): Problem | null {
  const dimension = (value: unknown) => isIntIn(value, 1, IMAGE_LIMITS.maxDimension)
  switch (operation) {
    case 'convert':
      return isIntIn(options.convert.quality, 1, 100) ? null : { key: 'image.invalid.quality' }
    case 'compress':
      return isIntIn(options.compress.quality, 1, 100) ? null : { key: 'image.invalid.quality' }
    case 'resize': {
      const resize = options.resize
      if (resize.mode === 'percent') {
        return isIntIn(resize.percent, 1, IMAGE_LIMITS.maxPercent)
          ? null
          : { key: 'image.invalid.percent', params: { max: IMAGE_LIMITS.maxPercent } }
      }
      return dimension(resize.width) && dimension(resize.height)
        ? null
        : { key: 'image.invalid.dimension', params: { max: IMAGE_LIMITS.maxDimension } }
    }
    case 'crop': {
      const { x, y, width, height } = options.crop
      const max = IMAGE_LIMITS.maxDimension
      if (!isIntIn(x, 0, max) || !isIntIn(y, 0, max) || !dimension(width) || !dimension(height)) {
        return { key: 'image.invalid.dimension', params: { max } }
      }
      const outside = files.find(
        (f) => f.image && (x + width > f.image.width || y + height > f.image.height),
      )
      return outside?.image
        ? {
            key: 'image.invalid.cropOutside',
            params: { name: outside.name, width: outside.image.width, height: outside.image.height },
          }
        : null
    }
    case 'merge':
      return isIntIn(options.merge.spacing, 0, IMAGE_LIMITS.maxSpacing)
        ? null
        : { key: 'image.invalid.spacing', params: { max: IMAGE_LIMITS.maxSpacing } }
    case 'toPdf':
      return isIntIn(options.toPdf.marginMm, 0, IMAGE_LIMITS.maxPdfMarginMm)
        ? null
        : { key: 'image.invalid.margin', params: { max: IMAGE_LIMITS.maxPdfMarginMm } }
    case 'rotate':
    case 'flip':
    case 'stripMetadata':
      return null
  }
}

/** Requests for the backend: one job per file, or a single job when joining images. */
export function buildImageRequests(
  operation: ImageOperation,
  files: FileInfo[],
  options: ImageOptions,
): JobRequest[] {
  const each = (make: (input: string) => JobRequest) => files.map((file) => make(file.path))
  switch (operation) {
    case 'convert': {
      const { format, quality } = options.convert
      return each((input) => ({
        type: 'imageConvert',
        input,
        format,
        ...(format === 'jpeg' ? { quality } : {}),
      }))
    }
    case 'resize':
      return each((input) => ({ type: 'imageResize', input, resize: { ...options.resize } }))
    case 'crop':
      return each((input) => ({ type: 'imageCrop', input, ...options.crop }))
    case 'rotate':
      return each((input) => ({ type: 'imageRotate', input, rotation: options.rotate.rotation }))
    case 'flip':
      return each((input) => ({ type: 'imageFlip', input, axis: options.flip.axis }))
    case 'compress':
      return each((input) => ({ type: 'imageCompress', input, quality: options.compress.quality }))
    case 'stripMetadata':
      return each((input) => ({ type: 'imageStripMetadata', input }))
    case 'merge':
      return [{ type: 'imageMerge', inputs: files.map((f) => f.path), ...options.merge }]
    case 'toPdf':
      return [{ type: 'imagesToPdf', inputs: files.map((f) => f.path), ...options.toPdf }]
  }
}

/** Crop area covering the whole image, used as a starting point. */
export function fullArea(file: FileInfo | undefined): CropArea | null {
  return file?.image ? { x: 0, y: 0, width: file.image.width, height: file.image.height } : null
}
