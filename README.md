# Hearth

Hearth is a shared, always-on execution environment for constructing
3D virtual spaces from the inside.

[Come join our Discord server!](https://discord.gg/gzzJ3pWCft)

# The History of Virtual Worlds

Shared virtual spaces have been around for decades, in many forms. Before PCs
were capable of 3D graphics, a popular kind of virtual space were multi-user
dungeons, or MUDs. Users can connect to MUDs from a text-based client like
telnet, and join other users in a textual virtual world. Although most MUDs
only have server-provided worlds that constrained users into their preset
rules, some MUDs (such as MUCKs and MOOs) allow users to extend the world with
their own functionality. In the early 2000s, Second Life implemented the same
principles but in a 3D space instead of a textual one. Users can create their
own spatial virtual worlds or enter other users' worlds with a 3D avatar. In
the modern day, platforms such as Roblox, VRChat, Rec Room, and Neos all
perform the same basic task, but in virtual reality. The decades-old
commonality between all of these diversity platforms is user-created content.
What's next?

# Philosophy

Hearth is a proof-of-concept of a new design philosophy for constructing shared
virtual spaces based on three fundamental design principles:

1. All content in the space can be extended and modified at runtime. This
  includes models, avatars, textures, sounds, UIs, and so on. Importantly,
  scripts can also be loaded at runtime, so that the behavior of the space
  itself can be extended and modified.
2. The space can pull content from outside sources. The space can load data
  from a user's filesystem or the Internet, and new scripts can be written to
  support loading unrecognized formats into the space.
3. The space itself can be used to create content. Tooling for creating assets
  for the space is provided by the space itself and by scripts extending that
  tooling.

Following these principles, a space can construct a content feedback loop that
can be fed by outside sources. Users can create their own content while
simultaneously improving their tooling to create content, all without ever
leaving the space. The result is an environment that can stay on perpetually
while accumulating more and more content. The space will grow in scale to
support any user's desires, and it can remix the creative content that already
exists on the Internet. This space has the potential to become a
next-generation form of traversing and defining the Internet in a collaborative
and infinitely adaptable way.

Hearth's objective is to create a minimalist implementation of these
principles, and find a shortest or near-shortest path to creating a
self-sustaining virtual space. To do this, the development loop between
script execution, content, and content authoring must be closed. This process
is analagous to bootstrapping an operating system, where once the initial
system is set up, the system itself can be used to expand itself. Once Hearth
has achieved this, the next goal will be to explore and research the
possibilities of the shared virtual space, to evaluate potential further use of
its design principles.

# Architecture

The name "Hearth" is meant to invoke the coziness of sharing a warm fireplace
with loved ones. Hearth is based on a straight-forward client-server network
architecture, with multiple peers connecting to a single persistent host. In
Hearth, all peers on the network are assumed to be friendly, so any peer can
perform any action on the world. This assumption sidesteps a massive amount of
design dilemmas that have plagued virtual spaces for as long as they have
existed.

In order to upgrade Hearth's design from a trustful private environment to a
trustless public service, future systems will need to solve these dilemmas in
order to prevent griefing. Hearth's goal is to explore implementations of
shared execution and content workflow, not security or permissions systems.

Hearth is effectively two programs at once: a distributed scripting
environment and a game engine. The scripting provides the metatextual rules of
the environment and the game engine creates the spatial substrate of the world.
The bridge between distributed execution logic and game engine logic is in the
scripting API. Scripts perform high-level logic in the environment, but are
also singularly responsible for loading and managing all of the spatial
content.

## Network

Hearth operates over TCP socket connections. Although other real-time
networking options like GameNetworkingSockets or QUIC have significantly
better performance, TCP is being used because it doesn't require NAT traversal
and it performs packet ordering and error checking without help from userspace.
When a connection to a Hearth server is made, the client and server will
perform a handshake to exchange encryption keys, and all further communication
will be  encrypted. Hearth assumes that peers are friendly, but not that
man-in-the-middle attacks are impossible. The specific encryption protocol is
TBD.

## Scripting

Scripting in Hearth is done with WebAssembly. Blah blah blah, lightweight spec,
blah blah blah, linear memory storage, blah blah blah, lots of runtime options,
I've said all of this before.

Hearth's execution model is inspired by BEAM, the virtual machine that the
Erlang and Elixir programming languages run on. BEAM is a tried-and-true
solution for creating resilient, fault-tolerant, hot-swappable systems, so many
of Hearth's design choices follow BEAM's. Hearth scripts are ran as green
thread "processes" that can spawn and kill other processes. Processes share
data by sending messages back and forth from each other. A significant
departure from BEAM is that because Hearth runs on multiple computers at once,
processes can spawn other processes on different peers, which can be either
another client or the server. Hearth's networking implementation will then
transport messages sent from a process on one peer to another, and the server
will relay client-to-client messages.

A working example of a BEAM-like WebAssembly runtime is Lunatic. Lunatic runs
green Wasm threads on top of a Rust asynchronous runtime, which handles all of
the cooperative multitasking and async IO. Lunatic also implements preemptive
multitasking by timeslicing Wasm execution, then yielding to other green
threads. Lunatic's operation is another large influence on Hearth's design, and
Hearth may reference--or even directly use--Lunatic's code in its own codebase.

## IPC

To administrate the network, every peer in a Hearth network exposes an IPC
interface on a Unix domain socket. This can be used to query running processes,
retrieve properties of the other peers on the network, send and receive
messages to processes, kill or restart misbehaving processes, and most
importantly, load new WebAssembly modules into the environment. The expected
content development loop in Hearth is to develop new scripts natively, then
load and execute the compiled module into Hearth using IPC.

## Rendering

## ECS

## Assets

Assets are multipurpose, hash-identified binary blobs that are exchanged
on-demand over the Hearth network. To distribute modules to other peers, Hearth
first hashes each asset and uses that hash as an identifier to other peers. If
a peer does not own a copy of an asset, it will not recognize the hash of that
asset in its cache, and it will request the asset's data from another peer.
Client peers request asset data from the server, and the server requests asset
data from the client that has referenced the unrecognized ID.

Before assets can be specialized for different kinds of content, they must be
loaded. Different asset classes, like meshes, textures, and WebAssembly modules
have different named identifiers. The names for those asset classes may be
`Mesh`, `Texture`, or `WebAssembly`, for example. Once an asset is loaded, it
may be passed into the engine in the places that expect a loaded asset, such as
in a mesh renderer component. Wasm processes are also spawned using a loaded
WebAssembly module asset as the executable source.

Processes may create their own assets in memory and read a foreign asset's data
back into process memory. In this way, processes may procedurally generate
runtime Hearth content into the space, load content data formats that the
core Hearth runtime does not recognize, or pipeline assets through multiple
processes that each perform some transformation on them.

Note that because Wasm modules are loaded from assets and because assets can be
dynamically created by processes, Hearth processes may generate and load Wasm
modules at runtime. This may be used, for example, in a WebAssembly compiler
running inside of Hearth itself.

Processes have the ability to send and receive the IDs of assets to other
processes. However, the runtime may not be aware that those IDs are being
exchanged, so a process running on a remote peer may receive an ID in a message
but the runtime will have no way of tracing it back to a peer that has that
ID's data. To remedy this, processes have a host call that explicitly warms a
target peer's asset cache with a given asset, and must use that host call when
referencing asset IDs to a remote process that does not have the asset.

## Terminal Emulator

So far, only Hearth's network architecture, execution model, and content model
have been described. These fulfill the first and second principles of Hearth's
design philosophy, but not the third principle: the space itself must provide
tooling to extend and modify itself. The easiest and simplest way to do this is
to implement a virtual terminal emulator inside of the 3D space, as a floating
window. This terminal emulator runs a native shell on the hosting user's
computer, with full access to the filesystem, native programs, and the IPC
interface for the Hearth process. The user can use the virtual terminal to edit
Hearth scripts using an existing terminal text editor like Vim, Neovim, or
Emacs, compile the script into a WebAssembly module, then load and execute that
module, all without ever switching from Hearth to another application or
shutting down Hearth itself.

The terminal emulator's text is rendered with multichannel signed distance
field (MSDF) rendering. MSDF rendering is good for text in 3D space because
each glyph can be drawn with high-quality antialiasing and texture filtering
from a large variety of viewing angles.

## Platform

Hearth runs outside of the browser as a native application. Linux is the only
target platform to start with but Windows support will also be added during
the [alpha phase of development](#phase-2-alpha).

# Usecases

# Implementation

Hearth is written entirely in Rust. Rust rust rust rust rust rust rust. It's
licensed under the GNU Affero General Public License version 3 (AGPLv3). Beer.
Beeeeeeeeeeeeeeeeeeeeeeer. Freedom.

## Networking

The [Tokio](https://tokio.rs) provides an asynchronous Rust runtime for async
code, non-blocking IO over both TCP and Unix domain sockets, and a foundation
for WebAssembly multitasking.

The protocols for both IPC and client-server communication are based on the
[Remoc](https://docs.rs/remoc/) crate, which implements transport-agnostic
channels, binary blob transport, remote procedure calls (RPC), watchable data
collections, and other networking constructs. This will save a lot of
development time that would otherwise be spent writing networking primitives.
Using Remoc makes it difficult to standardize a top-to-bottom protocol
definition, and the assumption is being made that no programs other than Hearth
or its forks will be using the protocol. The development time saved is more
than worth the loss of interoperability.

## Rendering

Scenes are rendered with [rend3](https://github.com/BVE-Reborn/rend3), a
batteries-included rendering engine based on
[wgpu](https://github.com/gfx-rs/wgpu). rend3 includes PBR, lighting, a render
graph, skybox rendering, frustum culling, rigged mesh skinning, tone mapping,
and other handy renderer features that Hearth doesn't need to do itself.
Additionally, rend3's frame graph allows us to easily extend the renderer with
new features as needed.

## Terminal Emulator

The terminal emulator is implemented with the help of the
[alacritty_terminal](https://crates.io/crates/alacritty_terminal) crate, which
parses the output of native child processes and writes it into a display-ready
grid of characters. Then, Hearth draws the characters onto 2D planes projected
in 3D space using MSDF textures generated with the
[msdfgen](https://crates.io/crates/msdfgen) crate. This rendering is a custom
node in the rend3 frame graph with a custom shader.

- TBD: can we load glyph outlines from a TTF file without building Freetype?
  * note: the ttf-parser crate looks like what we need)
- TBD: UX for interacting with in-space terminals?

## Input

## WebAssembly

## TUIs

## CLIs

# Roadmap

## Phase 0: Pre-Production

In phase 0, Hearth documents its purpose, proposes an implementation, decides
on which libraries and resources to use in its development, and finds a handful
of core developers who understand Hearth's goals and who are capable of
meaningfully contributing in the long run.

- [ ] Write a design document
- [x] Create a Discord server
- [x] Create a GitHub repository
- [ ] Onboard 3-4 core developers who can contribute to Hearth long-term
- [ ] Design a project logo
- [ ] Design a workspace structure
- [ ] Finalize the rest of the phases of the roadmap
- [ ] Create mocks for all of the codebase components
- [ ] Money?

## Phase 1: Pre-Alpha

Hearth's core host-side components can generally be decoupled from each other
into several different areas of development:

1. IPC and TUI interfaces.
2. Client-server networking.
3. Process management.
4. ECS integration.
5. Virtual terminal emulator development.

Because these different areas are independent, the early-stage development plan
for Hearth is to work on each of these areas in parallel. During this point of
development, it's important that multiple developers work in coordination with
each other in order to progress to beta as quickly as possible. Mock interfaces
and placeholder data where functioning inter-component code would otherwise go
are used to develop each component separately.

## Phase 2: Alpha

## Phase 3: Beta

## Phase 4: Release
