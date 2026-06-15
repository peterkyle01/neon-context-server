1. Create a Neon API key at https://console.neon.tech/app/settings?modal=create_api_key
2. Open Zed settings and configure `context_servers.neon-context-server.settings.neon_api_key`.
3. That's it! The project, branch, and database are auto-discovered at runtime.

All configuration can be done through the API key alone — the tools will prompt the agent to pick a project if there are multiple.
