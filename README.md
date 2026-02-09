# GitHub App Proxy

指定された GitHub App Installation として認証し、リクエストを GitHub API にプロキシするサーバーです。

## 概要

このサービスは、アプリケーションと GitHub (または GitHub Enterprise) の間に配置されます。設定された GitHub App Installation のアクセストークンを自動的に取得し、リクエストヘッダーに注入して GitHub へ転送します。これにより、クライアント側で GitHub App の複雑な認証フロー（JWT生成、トークン取得など）を実装する必要がなくなります。

## 機能

- **自動認証**: GitHub App の秘密鍵を使用して Installation Access Token を自動生成・管理します。
- **プロキシ**: 適切な認証情報 (`Authorization: Basic x-access-token:<token>`) を付与してリクエストを GitHub API に転送します。
- **MCP プロキシ**: `/mcp` で始まるリクエストを GitHub Copilot の Remote MCP Server (`https://api.githubcopilot.com/mcp/`) に転送します。認証には `Authorization: Bearer <token>` を使用します。
- **ヘルスチェック**: `/healthz` エンドポイントにより、サービスの稼働状況を確認できます。

## 設定

設定は環境変数で行います。

| 変数名 | 説明 | デフォルト | 必須 |
|----------|-------------|---------|----------|
| `GITHUB_APP_ID` | GitHub App の App ID | - | Yes |
| `GITHUB_APP_INSTALLATION_ID` | GitHub App の Installation ID | - | Yes |
| `GITHUB_APP_PRIVATE_KEY` | GitHub App の秘密鍵 (PEM形式 または Base64エンコードされた文字列) | - | Yes |
| `LISTEN_ADDR` | サーバーがリッスンするアドレスとポート | `0.0.0.0:8080` | No |
| `GIT_BASE_URL` | GitHub のベース URL (GitHub Enterprise の場合はその URL) | `https://github.com` | No |
| `GITHUB_API_PREFIX` | API のプレフィックス (例: `/api/v3`) | 自動判定 | No |
| `GITHUBCOPILOT_API_BASE` | GitHub Copilot API のベース URL | `https://api.githubcopilot.com` | No |

## 実行方法

### Docker で実行

1. `.env` ファイルを作成し、必要な環境変数を設定します（`docker.env.tmpl` を参考にしてください）。

```bash
docker build -t github-app-proxy .
docker run -p 8080:8080 --env-file .env github-app-proxy
```

### ローカルで実行 (Rust)

```bash
# 環境変数を設定して実行
export GITHUB_APP_ID=...
export GITHUB_APP_INSTALLATION_ID=...
export GITHUB_APP_PRIVATE_KEY=...
cargo run
```

## 仕組み

1. クライアントからリクエストを受信します。
2. GitHub App の秘密鍵を使って JWT を生成し、GitHub から Installation Access Token を取得します。
3. 取得したトークンを使って `Authorization` ヘッダーを構築します。
   - **GitHub API リクエスト**: `Basic` 認証スキームを使用し、ユーザー名を `x-access-token`、パスワードをトークンとしてエンコードします。
     - 例: `x-access-token:ghs_...` を Base64 エンコード
   - **MCP リクエスト** (`/mcp` で始まるパス): `Bearer` 認証スキームを使用します。
     - 例: `Bearer ghs_...`
4. リクエストをターゲットの GitHub API または GitHub Copilot MCP Server に転送し、レスポンスをクライアントに返します。
