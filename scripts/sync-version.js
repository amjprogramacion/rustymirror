#!/usr/bin/env node
/**
 * sync-version.js
 * Reads the version from package.json and propagates it to:
 *   - src-tauri/tauri.conf.json
 *   - src-tauri/Cargo.toml
 *   - package-lock.json
 *   - .env  (VITE_APP_VERSION)
 *
 * Run automatically via "predev" and "prebuild" hooks in package.json,
 * or manually: node scripts/sync-version.js
 */

import { readFileSync, writeFileSync } from 'fs'
import { execSync } from 'child_process'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = resolve(__dirname, '..')

// ── Read source of truth ──────────────────────────────────────────────────────
const pkg = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8'))
const version = pkg.version
console.log(`[sync-version] Syncing version ${version}`)

// ── tauri.conf.json ───────────────────────────────────────────────────────────
const tauriConfPath = resolve(root, 'src-tauri/tauri.conf.json')
const tauriConf = JSON.parse(readFileSync(tauriConfPath, 'utf8'))
tauriConf.version = version
writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n')
console.log(`[sync-version] Updated tauri.conf.json`)

// ── Cargo.toml ────────────────────────────────────────────────────────────────
const cargoPath = resolve(root, 'src-tauri/Cargo.toml')
let cargo = readFileSync(cargoPath, 'utf8')
// Only replace the first occurrence (the package version, not dependency versions)
cargo = cargo.replace(/^version = "[\d.]+"/m, `version = "${version}"`)
writeFileSync(cargoPath, cargo)
console.log(`[sync-version] Updated Cargo.toml`)

// ── Cargo.lock ────────────────────────────────────────────────────────────────
// Cargo.lock is auto-generated, so we let cargo update only the package entry.
const srcTauriPath = resolve(root, 'src-tauri')
execSync(`cargo update --precise ${version} --package rustymirror`, { cwd: srcTauriPath, stdio: 'inherit' })
console.log(`[sync-version] Updated Cargo.lock`)

// ── package-lock.json ─────────────────────────────────────────────────────────
const pkgLockPath = resolve(root, 'package-lock.json')
const pkgLock = JSON.parse(readFileSync(pkgLockPath, 'utf8'))
pkgLock.version = version
if (pkgLock.packages && pkgLock.packages['']) {
  pkgLock.packages[''].version = version
}
writeFileSync(pkgLockPath, JSON.stringify(pkgLock, null, 2) + '\n')
console.log(`[sync-version] Updated package-lock.json`)

// ── .env ──────────────────────────────────────────────────────────────────────
const envPath = resolve(root, '.env')
let env = ''
try { env = readFileSync(envPath, 'utf8') } catch {}
if (env.includes('VITE_APP_VERSION=')) {
  env = env.replace(/VITE_APP_VERSION=.*/m, `VITE_APP_VERSION=${version}`)
} else {
  env = env.trimEnd() + `\nVITE_APP_VERSION=${version}\n`
}
writeFileSync(envPath, env)
console.log(`[sync-version] Updated .env`)

console.log(`[sync-version] Done — all files set to ${version}`)
