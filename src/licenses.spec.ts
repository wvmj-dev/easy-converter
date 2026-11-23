// THIRD_PARTY_LICENSES.md must list the exact version of every direct dependency, so a
// version bump that forgets the file fails here.
import { describe, expect, it } from 'vitest'
import licenses from '../THIRD_PARTY_LICENSES.md?raw'
import packageJson from '../package.json?raw'
import packageLock from '../package-lock.json?raw'
import cargoLock from '../src-tauri/Cargo.lock?raw'
import cargoToml from '../src-tauri/Cargo.toml?raw'

const libraries = licenses.slice(licenses.indexOf('## Bibliotecas compiladas'))

function rustDependencies(): string[] {
  const section = cargoToml.split(/^\[dependencies\]$/m)[1]?.split(/^\[/m)[0] ?? ''
  return [...section.matchAll(/^([a-z0-9_-]+)\s*=/gm)].map((m) => m[1] ?? '')
}

function lockedVersions(name: string): string[] {
  const pattern = new RegExp(`\\[\\[package\\]\\]\\nname = "${name}"\\nversion = "([^"]+)"`, 'g')
  return [...cargoLock.matchAll(pattern)].map((m) => m[1] ?? '')
}

describe('THIRD_PARTY_LICENSES.md', () => {
  it('lists the version of every Rust dependency', () => {
    const deps = rustDependencies()
    expect(deps.length).toBeGreaterThan(10)
    const stale = deps.filter((name) => !lockedVersions(name).some((v) => libraries.includes(v)))
    expect(stale).toEqual([])
  })

  it('lists the version of every npm dependency', () => {
    const pkg = JSON.parse(packageJson) as { dependencies: Record<string, string> }
    const lock = JSON.parse(packageLock) as { packages: Record<string, { version?: string }> }
    const stale = Object.keys(pkg.dependencies).filter((name) => {
      const version = lock.packages[`node_modules/${name}`]?.version
      return !version || !libraries.includes(version)
    })
    expect(stale).toEqual([])
  })
})
