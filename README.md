# neon-context-server

Zed extension for [Neon](https://neon.tech) serverless Postgres to query databases, explore schemas, list projects and branches.

## Configuration

Requires a [Neon API key](https://console.neon.tech/app/settings?modal=create_api_key). Project, branch, and database are auto-discovered or you can give the assistant more info about which project/branch/database to query.

```json
{
  "context_servers": {
    "neon-context-server": {
      "settings": {
        "NEON_API_KEY": "napi_your_key_here"
      }
    }
  }
}
```

## Credits

Community extension by [Peter Kyle](peterkyle01.me).
