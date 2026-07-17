# Going further

You've built, drawn, beeped and shipped. What's left is depth, and it lives
in two places: the API docs and the repository's design documents.

## API documentation

The SDK's reference documentation is on docs.rs:
**[docs.rs/pixel8](https://docs.rs/pixel8)**. Everything this book showed —
and every alias, error type and edge case it glossed over — is documented
there, on the actual functions.

## The deeper documents

In [the repository's `docs/`](https://github.com/zeenix/pixel8/tree/main/docs):

| document | what's inside |
| -------- | ------------- |
| [ARCHITECTURE.md](https://github.com/zeenix/pixel8/blob/main/docs/ARCHITECTURE.md) | how the console is put together: one framebuffer, one rasterizer, the mode machine |
| [ABI.md](https://github.com/zeenix/pixel8/blob/main/docs/ABI.md) | the raw wasm import surface between cart and console — for the curious, and for anyone building an alternate SDK |
| [LIMITS.md](https://github.com/zeenix/pixel8/blob/main/docs/LIMITS.md) | exact accounting of the 128 K budgets |
| [CART_FORMAT.md](https://github.com/zeenix/pixel8/blob/main/docs/CART_FORMAT.md) | how a PNG cartridge is laid out, chunk by chunk |
| [WEB_EXPORT.md](https://github.com/zeenix/pixel8/blob/main/docs/WEB_EXPORT.md) | how the single-file web player works |
| [PICO8_IMPORT.md](https://github.com/zeenix/pixel8/blob/main/docs/PICO8_IMPORT.md) | the PICO-8 asset mapping, in detail |
| [HANDHELD.md](https://github.com/zeenix/pixel8/blob/main/docs/HANDHELD.md) | putting the player on retro handhelds |
| [TUI.md](https://github.com/zeenix/pixel8/blob/main/docs/TUI.md) | the terminal frontend: sixels, input protocols, tuning |

## The sandbox, in one paragraph

Since you'll wonder eventually: carts execute inside
[wasmi](https://github.com/wasmi-labs/wasmi) with no WASI, no filesystem, no
network and no host memory access. The only imports a cart gets are the small
C-like functions of the Pixel8 ABI — draw, input, audio, map, storage, log.
Fuel metering turns infinite loops into a friendly error screen. That's why
running a stranger's cartridge is a safe thing to do, and why carts from
today will still run bit-identically wherever the runtime goes next.

## Contributing

Pixel8 is free software (GPL-3.0-or-later) and welcomes contributions —
bug reports, carts, docs, code. Start with
[CONTRIBUTING.md](https://github.com/zeenix/pixel8/blob/main/CONTRIBUTING.md).
And if you make something with it: the whole point of PNG cartridges is that
they're easy to share. Share them.
