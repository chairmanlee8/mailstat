# mailstat

Command-line tool for analyzing an email inbox and producing various statistics.

With Gmail (see notes on Gmail):
```shell
cargo run -- --email ***@gmail.com --cache tmp/***.json --days 14
```

With Protonmail Bridge:
```shell
cargo run -- --email ***@proton.me --cache var/***.json --days 14 \
    --imap-host 127.0.0.1 --imap-port 1143 --imap-starttls \
    --smtp-host 127.0.0.1 --smtp-port 102
```

## TODO

- Fork himalaya->imap->imap-proto to support stupid mu character in ProtonMail bridge
- Complain about the above to protonmail bridge repo
- Figure out how to work launchd

## Issues

- DateTime parsing is wrong, value from Gmail is actually Utc not Local

## Gmail

Must create a dedicated app password for use with less-secure apps on Gmail.  Prefer the use of a shell tool to manage
passwords such as `pass` (https://www.passwordstore.org/).  The default shell command that mailstat uses is 
`pass show mailstat/<email>`.

Ensure that GPG_TTY is set so that the password input TUI can be shown.

```
GPG_TTY=$(tty)
export GPG_TTY
```

- See [https://github.com/soywod/himalaya/issues/442](#442)
- See [https://github.com/soywod/himalaya/issues/377](#377)
