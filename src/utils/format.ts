const UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

/** Human-readable size using powers of 1024, with the locale's number format. */
export function formatBytes(bytes: number, locale: string): string {
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024
    unit += 1
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1
  const number = new Intl.NumberFormat(locale, {
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  }).format(value)
  return `${number} ${UNITS[unit]}`
}

/** Last component of a path, for both `/` and `\` separators. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/)
  return parts[parts.length - 1] || path
}
