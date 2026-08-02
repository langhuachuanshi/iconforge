#!/usr/bin/env node
/**
 * 版本号同步脚本（唯一来源：package.json）
 *
 * 用法：pnpm run bump <版本号>
 *   例：pnpm run bump 0.2.0
 *
 * 把 package.json 的版本号同步写到：
 *   1. src-tauri/Cargo.toml      （package.version）
 *   2. src-tauri/tauri.conf.json （version 字段）
 *
 * Cargo.lock 不用手动改 —— 下次 cargo build 会自动跟着 Cargo.toml 更新。
 * 本脚本只改文件，不做 git commit / tag，由使用者自行提交。
 */
import { readFileSync, writeFileSync } from 'node:fs'
import { execSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const ROOT = dirname(fileURLToPath(import.meta.url))
const PKG_PATH = join(ROOT, '..', 'package.json')
const CARGO_PATH = join(ROOT, '..', 'src-tauri', 'Cargo.toml')
const TAURI_PATH = join(ROOT, '..', 'src-tauri', 'tauri.conf.json')

// 校验语义化版本号（仅校验格式，不校验范围）
const SEMVER_RE = /^\d+\.\d+\.\d+(?:-[\w.]+)?(?:\+[\w.]+)?$/

const arg = process.argv[2]
if (!arg) {
  console.error('用法:')
  console.error('  pnpm run bump <patch|minor|major>   自动算下一版本号')
  console.error('  pnpm run bump <x.y.z>               精确指定版本号')
  console.error('例: pnpm run bump minor   /   pnpm run bump 0.2.0')
  process.exit(1)
}

// 先读 package.json（参数解析需要当前版本号来递增）
const pkgRaw = readFileSync(PKG_PATH, 'utf8')
const pkg = JSON.parse(pkgRaw)

// 解析版本号：patch/minor/major 自动递增，否则按精确值校验
let version
if (['patch', 'minor', 'major'].includes(arg)) {
  const cur = pkg.version.split('.').map(Number)
  if (cur.length !== 3 || cur.some(Number.isNaN)) {
    console.error(`当前 package.json 版本号 "${pkg.version}" 非 x.y.z 格式，无法自动递增`)
    process.exit(1)
  }
  if (arg === 'patch') cur[2]++
  else if (arg === 'minor') { cur[1]++; cur[2] = 0 }
  else { cur[0]++; cur[1] = 0; cur[2] = 0 } // major
  version = cur.join('.')
} else {
  version = arg
  if (!SEMVER_RE.test(version)) {
    console.error(`非法版本号 "${version}"，应为 x.y.z 格式（如 0.2.0）或 patch/minor/major`)
    process.exit(1)
  }
}

// 1. package.json —— 写入新版本号（pkg 已在上方读取）
const oldVersion = pkg.version
pkg.version = version
writeFileSync(PKG_PATH, JSON.stringify(pkg, null, 2) + '\n', 'utf8')

// 2. Cargo.toml —— 只替换 [package] 段下的 version = "..."
const cargoRaw = readFileSync(CARGO_PATH, 'utf8')
const cargoRe = /^(\[package\][\s\S]*?version\s*=\s*")[^"]*(")/m
if (!cargoRe.test(cargoRaw)) {
  console.error(`Cargo.toml: 找不到 [package] 段的 version 字段`)
  process.exit(1)
}
writeFileSync(CARGO_PATH, cargoRaw.replace(cargoRe, `$1${version}$2`), 'utf8')

// 3. tauri.conf.json —— 只替换顶层 version 字段，不重新序列化整个文件
//    （整文件序列化会改动无关数组的缩进格式，只做字符串替换更安全）
const tauriRaw = readFileSync(TAURI_PATH, 'utf8')
const tauri = JSON.parse(tauriRaw)
if (typeof tauri.version !== 'string') {
  console.error(`tauri.conf.json: 找不到顶层 version 字段`)
  process.exit(1)
}
// 匹配 "version": "..." 仅在第一个出现的顶层位置（version 是 productName 后的第一个字段）
if (!tauriRaw.includes(`"version": "${tauri.version}"`)) {
  console.error(`tauri.conf.json: 定位 version 字段失败`)
  process.exit(1)
}
writeFileSync(
  TAURI_PATH,
  tauriRaw.replace(`"version": "${tauri.version}"`, `"version": "${version}"`),
  'utf8',
)

console.log(`版本号已同步: ${oldVersion} → ${version}`)
console.log('  ✓ package.json')
console.log('  ✓ src-tauri/Cargo.toml')
console.log('  ✓ src-tauri/tauri.conf.json')
console.log('（Cargo.lock 将在下次 cargo build 时自动更新，无需手动改）')

// 4. git commit + 打 tag + push（push tag 后会触发 release CI）
const tagName = `v${version}`
const FILES = ['package.json', 'src-tauri/Cargo.toml', 'src-tauri/tauri.conf.json']
console.log(`\n提交并打 tag ${tagName} ...`)
try {
  execSync(`git add ${FILES.join(' ')}`, { stdio: 'inherit' })
  execSync(`git commit -m "chore: release ${tagName}"`, { stdio: 'inherit' })
  execSync(`git tag ${tagName}`, { stdio: 'inherit' })
  execSync('git push && git push --tags', { stdio: 'inherit' })
  console.log(`\n✓ 已推送 tag ${tagName}，release CI 应已触发`)
  console.log('  在 GitHub 仓库 Actions 页查看构建进度，构建完成后 MSI 会出现在 Releases。')
} catch (e) {
  console.error('\n✗ git 操作失败（版本号文件已改，请手动处理）：')
  console.error(`  git add ${FILES.join(' ')}`)
  console.error(`  git commit -m "chore: release ${tagName}"`)
  console.error(`  git tag ${tagName}`)
  console.error('  git push && git push --tags')
  process.exit(1)
}
