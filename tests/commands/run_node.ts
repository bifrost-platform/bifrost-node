/**
 * run a new boot node
 */
async function runNode() {
  const yargs = require('yargs/yargs');
  const { hideBin } = require('yargs/helpers');

  const shell = require('shelljs');

  const argv = yargs(hideBin(process.argv))
    .usage('Usage: npm run run_node [args]')
    .version('1.0.0')
    .options({
      index: {
        type: 'number', describe: 'Node index to execute (The genesis node is 1)', default: 1,
      },
    }).help().argv;

  let port: number = 30333;
  let wsPort: number = 9945;
  let rpcPort: number = 9933;

  if (isNaN(Number(argv.index))) {
    console.error('please pass a numeric node index');
    return;
  }
  if (Number(argv.index) < 1) {
    console.error('please pass a positive numeric node index');
    return;
  }
  console.log(`[*] passed node index = ${argv.index}`);

  const basePath: string = `../data/boot${argv.index}`;
  console.log(`[*] node base path = ${basePath}`);

  // build_spec.sh
  shell.exec('scripts/build_spec.sh');
  console.log('[*] build chain spec');

  // main boot node
  if (argv.index === 1) {
    // set_main_node.sh
    shell.exec(`scripts/set_main_node.sh ${basePath}`);
    console.log('[*] set boot node');
  }

  port += argv.index;
  wsPort += argv.index;
  rpcPort += argv.index;

  // run_boot_node.sh
  shell.exec(`scripts/run_boot_node.sh ${argv.index} ${port} ${wsPort} ${rpcPort} ${basePath}`);
  console.log('[*] boot node initialized');
}
runNode();
