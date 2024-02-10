# Jari (Just a Radio by Individuals)

## Development Environment (Nix Flake)
To start developing and contributing to the project, it is recommended
to configure your system to use the development shell provided by the
[Nix flake](https://github.com/sk4rd/jari/blob/main/flake.nix). The
following sections will guide you on how to install and configure Nix,
to make use of the flake.

### Nix Installation
This section describes the process of setting up the Nix package
manager on your OS.

#### Linux
[Installing Nix on your Linux
distro](https://nixos.org/download#nix-install-linux) is
straightforward. It is recommended to install it in the 'Multi-user'
configuration. Make sure you have **systemd** on your system and
**SELinux** is disabled.

To install Nix, run the following command in your terminal:

```
sh <(curl -L https://nixos.org/nix/install) --daemon
```

You may have to install the `curl` command if it's not already
installed on your system:

```
sudo apt install -y curl
```

#### Windows 10/11 (WSL2)
In order to [install Nix on a Windows
machine](https://nixos.org/download#nix-install-windows), WSL (Windows
Subsystem for Linux) is required. Ensure you have at least Windows
build **1945** installed. Check your build version using the following
PowerShell or CMD snippet:

```
systeminfo | findstr /B /C:"OS Name" /B /C:"OS Version"
```

If your build number is greater than **1945**, you can proceed with
the automatic installation. Otherwise, you'll have to set up WSL
manually or update Windows.

##### Automatic
The [automatic setup of
WSL2](https://learn.microsoft.com/en-us/windows/wsl/install) is
simple. By running the following snippet in your terminal, Ubuntu will
be installed as your default Linux distribution.

```
wsl --install
```

The distro setup will ask you for a username and password. Remember,
that the password will not be echoed.

Next, make sure to install Nix by running the following command in
your terminal:

```
sh <(curl -L https://nixos.org/nix/install) --daemon
```

Once the setup finishes, you can start using Nix after you've reloaded
your shell.

##### Manual
In order to set up WSL2 manually on older builds of windows, please
refer to the [Microsoft
documentation](https://learn.microsoft.com/en-us/windows/wsl/install-manual).

### Using the Flake
In Nix, flakes are still an experimental feature. To use our flake,
you'll have to enable `flakes` and preferably `nix-command`. You can
achieve this by editing the Nix config file.

Create the Nix config directory, if it doesn't already exist:

```
mkdir -p ~/.config/nix
```

After you've created the directory run the following command, to
enable `flakes` and `nix-command`:

```
echo "experimental-features = flakes nix-command" > ~/.config/nix/nix.conf
```

Now you'll be able to use the new `nix` command and take advantage of
flakes.

#### Entering the DevShell
To enter the development shell, navigate to the *jari* project
directory:

```
cd jari/
```

Then, simply run the following command:

```
nix develop .
```

The above will have put you in the development shell and you're now
ready to hack away!

#### Automatic DevShell Entry (Recommended)
Running `nix develop .` can get quite tedious after some time. To
mitigate having to manually enter the development shell, we can use
[direnv](https://github.com/direnv/direnv) with the
[nix-direnv](https://github.com/nix-community/nix-direnv) addon. The
program will automatically source the flakes' shell and ensure that
you're all set up.

First we'll have to install `direnv` and `nix-direnv` using our newly
enabled `nix` command:

```
nix profile install nixpkgs#direnv nixpkgs#nix-direnv
```

After that, direnv needs to be configured to use the nix-direnv
'plugin':

```
mkdir -p ~/.config/direnv;
echo "source \$HOME/.nix-profile/share/nix-direnv/direnvrc" > ~/.config/direnv/direnvrc
```

Now you're ready to use `direnv`! You may enter the *jari* directory
and allow the flake to be sourced automatically:

```
direnv allow .
```

