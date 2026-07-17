# The Pixel8 Book

A hands-on tutorial for Pixel8, built with [mdBook](https://rust-lang.github.io/mdBook/)
and published to <https://zeenix.github.io/pixel8/> by
[`.github/workflows/deploy-site.yml`](../.github/workflows/deploy-site.yml)
on every push to `main`.

## Building locally

For the book alone, with live reload:

```sh
cargo install mdbook
mdbook serve book        # from the repo root
```

Chapters embed the example carts as `<iframe src="play/....html">` and the
editor screenshots as `shots/*.png`; both are built next to the book by the
site build, so in a book-only preview the frames and screenshots are empty.
To build and preview the full site — book, playable carts, screenshots,
shelf and all — use the same script the deploy workflow runs:

```sh
./scripts/build-site.sh site
python3 -m http.server -d site
```

When adding an example cart to the book, add it to the cart list in
`scripts/build-site.sh` and to the shelf in `scripts/build-index.sh`.
