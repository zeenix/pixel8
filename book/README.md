# The Pixel8 Book

A hands-on tutorial for Pixel8, built with
[mdBook](https://rust-lang.github.io/mdBook/) and published to
<https://zeenix.github.io/pixel8/>.

## Building locally

```sh
cargo install mdbook
mdbook serve book        # from the repo root; opens a live-reloading preview
```

Chapters embed the example carts as `<iframe src="play/....html">` and the
console/editor screenshots as `shots/*.png`; on the published site both are
built next to the book, so in a book-only preview the frames and screenshots
are empty.
