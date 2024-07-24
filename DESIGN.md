https://thume.ca/2020/04/18/telefork-forking-a-process-onto-a-different-computer/

Interface:

```shell
remote1> teleserver
...

remote2> some_process &
PID: 42
remote2> teleclient -p 42
```

teleserver and teleclient communicate over a network socket (HTTP?)
teleclient reads process information locally using `ptrace`, then sends a network request to teleserver
teleserver receives

Things that need to be copied over

- Memory
- Registers/processor state
- File descriptors
- Terminal information?
