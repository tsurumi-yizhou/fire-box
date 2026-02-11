# Fire Box

A stateless Rust LLM API gateway that translates and forwards OpenAI, Anthropic, and DashScope requests.

## Features

- Protocol adapters for OpenAI, Anthropic and DashScope.
- Streaming (SSE) and synchronous request handling.
- Routing and provider selection with fallback.
- In-memory file staging (uploads received at the channel layer are stored and lazily injected when sending to providers).
- DashScope refresh token rotation handling and per-provider persisted refresh token files.

## Quick start

1. Copy `sample.json` to `config.json` and fill in your provider/channel keys:

```sh
cp sample.json config.json
# edit config.json and replace placeholders
```

2. Build and run (development):

```sh
cargo run --release -- config.json
```

Or build a release binary:

```sh
cargo build --release
./target/release/fire-box config.json
```

## Important notes

- Do NOT commit `config.json` containing real API keys. Use `sample.json` as the template and keep your `config.json` local and out of version control.
- DashScope (Qwen) refresh tokens are single-use on exchange: the gateway persists rotated refresh tokens to files named `.dashscope_refresh_token_<provider_tag>` in the repository working directory. Ensure the initial `refresh_token` you place into `config.json` is fresh; the gateway will update the persisted file after a successful exchange.
- The repo currently contains `sample.json` (placeholders). If you have already accidentally committed secrets, rotate those credentials immediately and remove them from git history.

## Service unit

An example systemd unit is provided as `fire-box.service` for reference. Install and enable as appropriate for your system:

```sh
# copy service file to /etc/systemd/system/fire-box.service
# edit ExecStart path and WorkingDirectory
sudo systemctl daemon-reload
sudo systemctl enable --now fire-box
```

## License

This project is licensed under the Mozilla Public License 2.0 (MPL-2.0). See the `LICENSE` file in the repository root for the full terms.