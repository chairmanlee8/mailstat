# mailstat

Command-line tool for analyzing an email inbox and producing various statistics.

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
