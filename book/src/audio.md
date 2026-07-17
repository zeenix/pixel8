# Sound and music

Pixel8's synthesizer has four channels, shared by everything that makes
noise. Sounds come in two flavors:

- **SFX** — 64 slots, each a little step-sequence (32 steps, 8 waveforms),
  authored in the [sfx editor](console.md#the-five-editors).
- **Music** — 64 patterns that arrange sfx across the four channels,
  authored in the music editor.

Nothing is loaded, streamed or mixed by you: the game says *play sfx 3*, the
console does the rest.

## Playing sound effects

```rust
const JUMP: SfxId = SfxId::new(0).unwrap();   // const-checked: 64+ won't compile

fn update(&mut self, ctx: &mut Context) {
    if ctx.is_button_pressed(Button::O) {
        ctx.sfx(JUMP);            // plays on a free channel
    }
}
```

`ctx.sfx` picks a free channel automatically — the right default. When you
need manual control (say, an engine hum that must be replaceable), pin a
channel with `sfx_on(sfx, Channel::Channel2)` and silence it with
`sfx_stop(Channel::Channel2)`.

<div class="cart-embed">
<iframe src="play/sfx_demo.html" title="sfx_demo — playable cart" loading="lazy"></iframe>
<span class="cart-caption">sfx_demo: a four-pad soundboard — each arrow key plays a slot
(<a href="https://github.com/zeenix/pixel8/blob/main/examples/sfx_demo/src/lib.rs">source</a>).</span>
</div>

## Playing music

Music starts from a builder and hands you back a handle:

```rust
struct MyGame {
    music: Option<PlayingMusic>,
}

fn update(&mut self, ctx: &mut Context) {
    if ctx.is_button_pressed(Button::O) && self.music.is_none() {
        self.music = ctx
            .music(MusicId::new(0).unwrap())
            .fade_in(500)                       // milliseconds; optional
            .play()
            .ok();
    }
    if ctx.is_button_pressed(Button::X) {
        if let Some(m) = self.music.take() {
            m.fade_out(500).stop();             // or just drop it
        }
    }
}
```

The [`PlayingMusic`] handle *owns* the running song: dropping it stops the
music (fading out first if you armed `fade_out`). That plays beautifully with
Rust — store the handle in whatever state means "music should be playing",
and the song can never outlive it. The platformer keeps its game-over jingle
inside the `Ended` state variant; leaving that state drops the handle and the
jingle with it.

Only one song plays at a time: `play()` returns `Err(MusicBusy)` — carrying
your request back — if another is running. Stop that one first.

If sound effects keep stealing your melody's channels, reserve some at start:

```rust
ctx.music(song)
    .reserve_channels(Channel::Channel0 | Channel::Channel1)
    .play()
```

Auto-routed `ctx.sfx` calls will then avoid channels 0 and 1 while the music
plays.

<div class="cart-embed">
<iframe src="play/music_demo.html" title="music_demo — playable cart" loading="lazy"></iframe>
<span class="cart-caption">music_demo: <kbd>Z</kbd> fades a song in, <kbd>X</kbd> fades it out
(<a href="https://github.com/zeenix/pixel8/blob/main/examples/music_demo/src/lib.rs">source</a>).</span>
</div>

## Authoring the sounds

The editors are where the actual audio comes from, and the best way to learn
them is to poke at a finished cart: `load` the sfx_demo or music_demo cart
(or open `examples/sfx_demo` from a source checkout), press `Esc`, and walk
through its sfx and music editors to see how the sounds are built. Start
simple — a jump is a few steps of a square wave sliding down in pitch; a coin
is two short high notes. The 8 waveforms and 32 steps go a surprisingly long
way.

[`PlayingMusic`]: https://docs.rs/pixel8/latest/pixel8/struct.PlayingMusic.html
