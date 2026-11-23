import { describe, expect, it } from 'vitest'
import type { FileInfo } from '../types/FileInfo'
import type { ToolId } from '../types/ToolId'
import { buildArchiveRequests, defaultArchiveOptions, needsSevenZip, unavailableReason as archiveReason } from './archive'
import { buildEbookRequests, defaultEbookOptions, unavailableReason as ebookReason } from './ebook'
import { buildSubtitleRequests, defaultSubtitleOptions, parseOffset, validateOptions as validateSubtitle } from './subtitle'
import { buildVideoRequests, defaultVideoOptions, validateOptions as validateVideo } from './video'

function file(name: string, category: FileInfo['category']): FileInfo {
  return { path: `/f/${name}`, name, extension: name.split('.').pop() ?? null, size: 1, category, image: null, pdf: null, media: null }
}

const all = () => true
const without = (missing: ToolId) => (id: ToolId) => id !== missing

describe('subtitles', () => {
  it('parse shifts with a direction', () => {
    expect(parseOffset({ offset: '1,5', direction: 'later' })).toBe(1500)
    expect(parseOffset({ offset: '0:02.5', direction: 'earlier' })).toBe(-2500)
    expect(parseOffset({ offset: '0', direction: 'later' })).toBeNull()
    expect(parseOffset({ offset: '25:00:00', direction: 'later' })).toBeNull()
    expect(parseOffset({ offset: 'abc', direction: 'later' })).toBeNull()
  })

  it('refuse the same format, counting SSA as ASS', () => {
    const o = defaultSubtitleOptions()
    o.convert.format = 'ass'
    expect(validateSubtitle('convert', o, [file('a.ssa', 'subtitle')])?.key).toBe('subtitle.convert.same')
    expect(validateSubtitle('convert', o, [file('a.srt', 'subtitle')])).toBeNull()
    expect(validateSubtitle('shift', o, [])?.key).toBe('subtitle.shift.invalid')
    o.shift = { offset: '2', direction: 'earlier' }
    expect(buildSubtitleRequests('shift', [file('a.srt', 'subtitle')], o)).toEqual([
      { type: 'subtitleShift', input: '/f/a.srt', offsetMs: -2000 },
    ])
  })

  it('are embedded into videos from the video screen', () => {
    const o = defaultVideoOptions()
    const clip = [file('a.mp4', 'video')]
    expect(validateVideo('embedSubtitle', o, clip, null)?.key).toBe('video.embedSubtitle.chooseSubtitle')
    o.embedSubtitle = { subtitle: '/s/a.srt', language: 'por' }
    expect(validateVideo('embedSubtitle', o, clip, null)).toBeNull()
    expect(buildVideoRequests('embedSubtitle', clip, o)).toEqual([
      { type: 'videoEmbedSubtitle', input: '/f/a.mp4', subtitle: '/s/a.srt', language: 'por' },
    ])
    o.embedSubtitle.language = ''
    expect(buildVideoRequests('embedSubtitle', clip, o)[0]).not.toHaveProperty('language')
  })
})

describe('ebooks', () => {
  it('need Calibre and another format', () => {
    const o = defaultEbookOptions()
    expect(ebookReason([file('a.mobi', 'ebook')], o, without('calibre'))?.params).toEqual({ tool: 'Calibre' })
    expect(ebookReason([file('a.epub', 'ebook')], o, all)?.key).toBe('ebook.unavailable.same')
    o.format = 'azw3'
    expect(buildEbookRequests([file('a.epub', 'ebook')], o)).toEqual([
      { type: 'ebookConvert', input: '/f/a.epub', format: 'azw3' },
    ])
  })
})

describe('archives', () => {
  it('extract archives only, with 7-Zip for RAR, xz and bzip2', () => {
    expect(needsSevenZip('a.RAR') && needsSevenZip('a.tar.xz') && !needsSevenZip('a.tar.gz')).toBe(true)
    expect(archiveReason('extract', [file('a.zip', 'archive'), file('b.pdf', 'pdf')], all)?.key).toBe(
      'archive.unavailable.notArchive',
    )
    expect(archiveReason('extract', [file('a.rar', 'archive')], without('sevenZip'))?.params).toEqual({ tool: '7-Zip' })
    expect(archiveReason('extract', [file('a.zip', 'archive')], () => false)).toBeNull()
  })

  it('put any files into one archive', () => {
    const files = [file('a.pdf', 'pdf'), file('b.jpg', 'image')]
    expect(archiveReason('create', files, () => false)).toBeNull()
    const o = defaultArchiveOptions()
    o.create.format = 'sevenZ'
    expect(buildArchiveRequests('create', files, o)).toEqual([
      { type: 'archiveCreate', inputs: ['/f/a.pdf', '/f/b.jpg'], format: 'sevenZ' },
    ])
    expect(buildArchiveRequests('extract', [file('a.zip', 'archive')], o)).toEqual([
      { type: 'archiveExtract', input: '/f/a.zip' },
    ])
  })
})
