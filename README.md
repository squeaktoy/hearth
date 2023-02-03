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

Hearth spaces are password-protected. Hearth assumes that peers are friendly,
but not that all network nodes with access to the server can become peers, or
that man-in-the-middle attacks are impossible. When a connection to a Hearth
server is made, the client and server execute a password-authenticated
key-exchange (PAKE) protocol, then use the resulting session key to encrypt
all future communication between them.

The specific PAKE method used is
[Facebook's OPAQUE protocol](https://github.com/facebook/opaque-ke) with
[Argon2](https://en.wikipedia.org/wiki/Argon2) as the password hash derivation
function. OPAQUE has been audited by NCC Group and Argon2 is well-established
for password hashing and is highly popular. After exchanging session keys,
[ChaCha20](https://en.wikipedia.org/wiki/Salsa20) is directly applied to TCP
communication. ChaCha20 is another well-established cryptography protocol.

TODO: write about future cryptography plans.

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

## Lumps

Lumps are multipurpose, hash-identified binary blobs that are exchanged
on-demand over the Hearth network. To distribute lumps to other peers, Hearth
first hashes each lump and uses that hash as an identifier to other peers. If
a peer does not own a copy of a lump, it will not recognize the hash of that
lump in its cache, and it will request the lump's data from another peer.
Client peers request lump data from the server, and the server requests lump
data from the client that has referenced the unrecognized ID.

Processes may both create their own lump in memory and read a foreign lump's
data back into process memory. In this way, processes may procedurally generate
runtime Hearth content into the space, load content data formats that the
core Hearth runtime does not recognize, or pipeline lumps through multiple
processes that each perform some transformation on them.

Processes may send and receive the hashed IDs of lumps to other processes via
messages. When an ID is sent to a process on a remote peer, however, the
remote runtime may not recognize that a lump's ID has been referenced. The
result is that the remote process obtains an ID for a lump but no way to access
its data. To remedy this, processes have a host call that explicitly transfers
a lump's data to a remote peer. This way, processes can ensure that remote
processes have access to lumps that are being transferred.

## Assets

Before lumps can be specialized for different kinds of content, they must be
loaded into assets. Different asset classes, like meshes, textures, and
WebAssembly modules have different named identifiers. The names for those asset
classes may be `Mesh`, `Texture`, or `WebAssembly`, for example. An asset is
loaded with the name of the asset class and the lump containing that asset's
data. Once an asset is loaded, it may be passed into the engine in the places
that expect a loaded asset, such as in a mesh renderer component. Wasm
processes are also spawned using a loaded WebAssembly module asset as the
executable source.

Assets are peer-local and there is no way to transfer them between peers. This
is because they have been specialized from a non-specialized binary blob into
a specialized data format that may or may not be peer-specific. Processes have
the responsibility and the privilege of converting opaque lumps into usable
assets on their host peer.

Note that lumps (and therefore assets) can be created by processes and that
Wasm modules are a kind of asset. As a consequence of this, Hearth processes
may generate and load Wasm modules at runtime. This allows a WebAssembly
compiler that can create new Hearth processes to be in of itself a Wasm Hearth
process. A major field of research in Hearth's [beta phase](#phase-3-beta) is
to study the possibilities of a self-hosting Hearth environment using this
technique.

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

### Loading Lumps

### Logging From Wasm

## CLI

Hearth provides a command-line interface (CLI) utility program to perform
simple interactions with the daemon. This may be invoked with an interactive
shell or by shell scripts. Scripts may chain together multiple invocations of
the CLI to create more complex workflows. This allows Hearth users to
repeatedly perform complex actions without needing to compile and spawn a
new Hearth process or to compile and run a new native process.

The CLI program is named `hearth-ctl` and has multiple subcommands that each
perform a different operation on the runtime. Hearth may add more subcommands
as needed, but at the current time of writing it is known that they include:

- `load-lump`: load a lump
- `list-processes`: list processes
- `kill`: kill a process
- `spawn`: spawn a process
- `tail`: tail a process's log
- `send`: send a process a message

`hearth-ctl` follows POSIX-like conventions for user interaction. It returns
reasonable exit codes according to the POSIX standard, and displays output to
`stdout` in lightly- or un-formatted strings that can be easily processed by a
shell script.

Exit codes are provided by the
[yacexits](https://crates.io/crates/yacexits) crate.

## TUI

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
- [ ] Set up continuous integration to check pull requests
- [ ] Write a CONTRIBUTORS.md describing contribution workflow
- [ ] Design a workspace structure
- [ ] Finalize the rest of the phases of the roadmap
- [ ] Create mocks for all of the codebase components
- [ ] Money?

## Phase 1: Pre-Alpha

In phase 1, each subsystem of Hearth is developed, and the details of its
design aspects are made concrete. The whole system has not yet been tied
together, and low-level design decisions are considered in isolation of each
other.

Hearth's core host-side components can generally be decoupled from each other
into several different areas of development or subsystems:

1. IPC, TUI, and CLI interfaces.
2. Client-server networking.
3. Process management.
4. ECS integration.
5. Virtual terminal emulator development.

Because these different areas are independent, the goal is to work on each of
these areas in parallel. During this point of development, it's important that
multiple developers work in coordination with each other in order to progress
to alpha as quickly as possible. Mock interfaces and placeholder data where
functioning inter-component code would otherwise go are used to develop each
component separately.

## Phase 2: Alpha

In phase 2, Hearth begins to come together as a whole. Each subsystem is hooked
into the others, and the developers work together to synthesize their work into
a single functioning application. Although at this point in development network
servers are started up for testing, the protocols between subsystems are
highly unstable, so long-lived, self-sustaining virtual spaces are still
unfeasible.

## Phase 3: Beta

In phase 3, Hearth's protocols and system interfaces are mature and relatively
stable, so a long-lived development space is created. In this space, developers
work together on exploring the capabilities of Hearth processes, and implement
practical applications in Hearth using Hearth's fundamental toolkit. If
oversights or missing features are found in Hearth's interfaces, they are
addressed as fit. However, because the fundamentals of Hearth's implementation
are complete, changes to interfaces are infrequent and often non-breaking.

A major focus of this phase is to refine the design principles of writing
Hearth processes through rapid iteration, collaboration, and peer review. This
makes phase 3 the most difficult phase to complete, as Hearth's goal during
this step is to explore uncharted design territory in a unique execution
environment.

## Phase 4: Release
