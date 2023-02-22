# Contributors

Thank you

# How To Use Git

Hearth uses Git for its version control system (VCS). To contribute code or
documentation to the main code repository, use Git to clone it locally, make
branches, commit changes, then push the branches back to GitHub and open a pull
request to begin the review process. If you're a member of the Hearth GitHub
organization, you can push new branches directly to the upstream repository.
If not, you'll need to fork the repository to your own account before you can
make changes.

Basic Git usage is out of the scope of this document, but there are plenty of
resources online on how to use Git, even if you have zero experience with it
or don't know how to code. Also feel free to ask for help on using Git on
our [Discord server](https://discord.gg/gzzJ3pWCft)!

For a pull request to be merged, our continuous integration tests must pass on
the introduced code. This is to ensure that our main branch always compiles and
runs as intended. You may need to push new commits to a pull request after it's
opened in order to address the errors raised by our testing.

If this is your first time contributing to Hearth, feel free to add a commit
to add your name and info to [the contributors section of this document](#contributors)!
This is a permanent record of your assistance on the project and adds your
copyright to the code's [licensing](#licensing).

# Naming Conventions

When creating a new branch we use a short but appropriate `kebab-case` phrase
to name them. Examples: `peer-api`, `nonzero-iv`, `lump-asset-loading`.

When writing a commit message, issue title, or pull request title, we prefix
the summary of the changes involved with the location of those changes. More
often than not the changes are in source code, so the location is a crate, such
as `rend3-alacritty`, `font-mud`, or `hearth-core`. Because so many crates
begin with `hearth-`, we omit them in locations. `rend3-alacritty` is also
abridged to `alacritty`. When the changes are in a Markdown document in the
repository root the location is the lowercase name of that file. If the changes
occur in more than one location, omit the location and write only the summary,
beginning with a capital letter.

Example titles:
- commit in `hearth-core`: `core: add ProcessFactoryImpl`
- commit in `hearth-client`: `client: add cognito dep and WasmPlugin`
- commit in `rend3-alacritty`: `alacritty: merge shaders into one file`
- change in `README.md`: `readme: break out design document into separate file`
- issue for changes in more than one location: `License everything`

# Writing Commits

The number one guideline to follow while writing commits is to contain all of
the changes introduced by a commit to a single location. Commits should NEVER
touch multiple locations simultaneously. When moving code from one location to
another, break up the commit by making one commit to add the code to the new
location and another commit to remove the code from the old location. When
modifying an API between crates first make the commit to change the API and
then make a new commit to update the API usage for each affected crate.
Although all pull requests must pass continuous integration before they can be
merged, it is alright to introduce some commits that will not build. The
purpose of organizing commits so rigorously is to keep a comprehensive log of
all changes made to each individual crate to ease the release and changelog
creation process.

Please also keep commits as small as possible. Large commits with hundreds of
new lines of code are acceptable as long as they do not affect a significant
portion of the codebase. However, when multiple changes are made affecting
less-related portions of the codebase, break up the relevant changes into
multiple commits with regards to the scope. This helps keep commit messages
small and informative, so that the commit diff does not need to be read in
order to determine the consequences of a commit.

# Coding Style

When in doubt, run `cargo fmt`.

# Writing Log Messages

# Licensing

- how to license files
- how the AGPL works
