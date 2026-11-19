export type Platform = 'linux' | 'windows'

export function currentPlatform(userAgent = navigator.userAgent): Platform {
  return /windows/i.test(userAgent) ? 'windows' : 'linux'
}
