# Project Structure

> まだコードが無いため、方針のみ記載します。実装開始後に `/kiro-steering` で同期します。

## Organization Philosophy

機能(feature)単位で分割し、Rust 側と WebView 側でそれぞれ同じ機能名のモジュールを持ちます。機能名は `.kiro/specs/` の spec 名に揃えます(app-shell / capture-library / screen-capture / annotation-editor)。

## Directory Patterns

### Rust コア
**Location**: `/src-tauri/src/<feature>/`
**Purpose**: 機能ごとのロジックと Tauri command。Windows API 呼び出しは `platform/` 配下の薄いアダプタに隔離する。
**Example**: `src-tauri/src/screen_capture/`, `src-tauri/src/platform/windows/`

### WebView UI
**Location**: `/src/features/<feature>/`
**Purpose**: 機能ごとの画面・コンポーネント・状態。
**Example**: `src/features/annotation-editor/`

### 共有型
**Location**: `/src/shared/`
**Purpose**: Rust と共有する IPC の型定義、共通 UI。

## Naming Conventions

- **Files (TS)**: コンポーネントは PascalCase、それ以外は kebab-case
- **Rust modules**: snake_case
- **Components**: PascalCase
- **Functions**: Rust は snake_case、TS は camelCase

## Import Organization

```typescript
import { Something } from '@/shared/ipc'          // 共有は絶対パス
import { Local } from './local'                     // 同一機能内は相対パス
```

**Path Aliases**:
- `@/`: `src/`

## Code Organization Principles

- 機能間の依存は spec の依存順(roadmap.md)に従う。下流の機能が上流を呼ぶ。
- Windows API への依存は platform アダプタに限定し、機能ロジックは OS 非依存の型で書く。
- WebView ↔ Rust の契約(command 名・payload 型)は `src/shared/` と Rust 側で一対一に対応させる。

---
_Document patterns, not file trees. New files following patterns shouldn't require updates_
