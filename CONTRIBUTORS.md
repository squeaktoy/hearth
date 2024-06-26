# Contributors

Thank you to everyone who has contributed to Hearth!

- Marceline Cramer, project creator (GitHub: [@marceline-cramer](https://github.com/marceline-cramer))
- Malek, early-stage code contributions (GitHub: [@MalekiRe](https://github.com/MalekiRe))
- Sasha Koshka, logo artist ([website](https://holanet.xyz))
- Emma Tebibyte, licensing and best practices consultant ([website](https://tebibyte.media/~emma))
- roux, frontend design and development (GitHub: [@airidaceae](https://github.com/airidaceae))

If this is your first time contributing to Hearth, feel free to to add your
name and info to this list! This is a permanent record of your assistance on
the project and explicitly adds your copyright to the code's
[licensing](#licensing).

# Finding Work

If you'd like to begin contributing to Hearth, the best place to look for
work to do is in the issue tracker on the GitHub repo. Issues always have the
most relevant tasks for the current state of the codebase. Issues tagged with
"question" are not ready to be implemented in code because their design is
incomplete or missing important information, so please consider reading those
issues and researching them. Issues tagged with "good first issue" are probably
complete design-wise and have relatively easy work required to complete them, so
if you're new to Hearth, this is the best work to do as you'll grow to be more
familiar in its codebase without needing to make major changes to it.

Generally speaking, any issue that does not have the "question" tag and that
does not have anyone assigned to it is fair game for code contribution. If you
decide to pick up an issue like that, please either self-assign it if you're in
the Hearth organization or leave a comment on it letting us know that it's
taken.

Issues tagged with "help wanted" are best for non-organization members because
we either do not know how to complete them or do not have the spare time. If
you see one that you'd like to work on, please let us know!

If you don't see any issues that you like, the next place to go is the roadmap.
You may see some incomplete items on it that may be adjacent to your skillset
and you think that you may have something to offer on a subsystem design level.
If this is the case, please join our
[Discord server](https://discord.gg/gzzJ3pWCft) and get in contact with us! We
would love to have you on our team with experienced hands and more points of
view on our diverse codebase. We can discuss design and architecture, add you
to the organization, and begin opening issues to plan out additions to the
codebase.

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

# Naming Conventions

When creating a new branch we use a short but appropriate `kebab-case` phrase
to name them. Examples: `peer-api`, `nonzero-iv`, `lump-asset-loading`.

When writing a commit message, issue title, or pull request title, we prefix
the summary of the changes involved with the location of those changes. More
often than not the changes are in source code, so the location is a crate, such
as `hearth-runtime`. Because all crates begin with with `hearth-`, we omit
that prefix in both their directory and our commit messages. When the changes
are in a Markdown document in the repository root the location is the lowercase
name of that file. If the changes occur in more than one location, omit the
location and write only the summary, beginning with a capital letter.

Example titles:
- commit in `runtime`: `runtime: add ProcessFactoryImpl`
- commit in `client`: `client: draw debug draw before terminal`
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

We're not picky on specific formatting, although if you don't format your code
with `rustfmt` or `cargo fmt`, our continuous integration checks will fail.
`rustfmt` has sole authority of code formatting.
