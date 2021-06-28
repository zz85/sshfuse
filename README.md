# sshfuse

Mirror a remote ssh host fs on on a fuse mount. this is a simplistic sshfs implementation in rust.

## Idea
The idea is to recreate a resemblance of a remote directory structure locally through the fuse file system,
by spawning a bunch of background detailed list ssh commands on a remote host,

## Instructions
You would need to install fuse on your system...

On Mac,

```
brew install --cask osxfuse
```

Debian based Linux
```
sudo apt-get install fuse
```

CentOS
```
sudo yum install fuse

```

## Example usage


```
sshfuse --user sshuser --target 123.123.123.123
```
### Supported use cases

- Read-only
- listing and navigating directories (`cd` and `ls` commands)
- syscalls (`getattr`, `readdir`, `opendir`)