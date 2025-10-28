# Functional testing for Bifrost Network

This folder contains a set of functional tests designed for Bifrost Network.

It is written in typescript, using Mocha/Chai as Test framework.

## Test flow

Each group will start a dev service with the
[development spec](../node/src/chain_spec.rs) before executing the tests.

## Test categories

- `tests`: Tests expected to run by spawning a new dev node (~1-2 minutes)
- `benchmark`: Benchmark of node transaction related action (Time depend on
  arguments)

## Installation

```
cd tests
npm install
```

## Run the tests

```
npm run test
```

## Run test commands

Run genesis validator node

```
npm run run_node -- --index 0
```

Run validator nodes (increase node index to add more nodes to the network)

```
npm run run_node -- --index 1
```

Set full validator nodes
```
npm run set_node -- \
  --index 1 \
  --provider ws://127.0.0.1:9944 \
  --full
```

Set basic validator nodes
```
npm run set_node -- \
  --index 1 \
  --provider ws://127.0.0.1:9944 \
```

Remove all persisted data

```
npm run purge_chains
```

## Run benchmark

```bash
# To know arguments, Run $ npm run benchmark -- --help
npm run benchmark -- <arguments>
```

## Run benchmark commands

After running benchmark, Docker containers still run continuously. Stop & Remove
all containers.

```bash
npm run purge_containers
```

# Debugging a Bifrost Node

The repository contains a pre-configured debugger configuration for VSCode with
the **CodeLLDB**
(`vadimcn.vscode-lldb`) extension.

Before debugging, you need to build the node with debug symbols with command
`RUSTFLAGS=-g cargo build --release` (available as a VSCode task). Then go in
the **Debug** tab in
the left bar of VSCode and make sure **Launch Bifrost Node (Linux)** is selected
in the top
dropdown. **Build & Launch Bifrost Node (Linux)** will trigger the build before
launching the node.

To launch the debug session click on the green "play" arrow next to the
dropdown. It will take some
time before the node starts, but the terminal containing the node output will
appear when it is
really starting. The node is listening on ports 19931 (p2p), 19932 (rpc) and
19933 (ws).

You can explore the code and place a breakpoint on a line by left clicking on
the left of the line
number. The execution will pause the next time this line is reached. The debug
toolbar contains the
following buttons :

- Resume/Pause : Resume the execution if paused, pause the execution at the
  current location
  (pretty random) if running.
- Step over : Resume the execution until next line, or go one level up if the
  end of the current
  scope is reached.
- Step into : Resume the execution to go inside the immediately next function
  call if any, otherwise
  step to next line.
- Step out : Resume the execution until the end of the scope is reached.
- Restart : Kill the program and start a new debugging session.
- Stop : Kill the program and end debugin session.

Breakpoints stay between debugging sessions. When multiple function calls are
made on the same line,
multiple step into, step out, step into, ... can be required to go inside one
of the chained
calls.

When paused, content of variables is showed in the debugging tab of VSCode. Some
basic types are
displayed correctly (primitive types, Vec, Arc) but more complex types such as
HashMap/BTreeMap
are not "smartly" displayed (content of the struct is shown by mapping is hidden
in the complexity
of the implementation).
