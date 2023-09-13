import shell from 'shelljs';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';

/**
 * run a new boot node
 */
async function runNode() {
  const argv = await yargs(hideBin(process.argv))
    .usage('Usage: npm run run_node [args]')
    .version('1.0.0')
    .options({
      index: {
        type: 'number',
        describe: 'Node index to execute (The genesis node is 0)',
        default: 0,
      },
      port: {
        type: 'number',
        describe: 'The P2P port used for networking. The default port is 30333.',
        default: 30333,
      },
      rpcPort: {
        type: 'number',
        describe: 'The RPC port used for Http and WebSocket connections. The default port is 9944.',
        default: 9944,
      }
    }).help().argv;

  if (isNaN(Number(argv.index))) {
    console.error('⚠️  Please enter a numeric node index.');
    return;
  }
  if (Number(argv.index) < 0) {
    console.error('⚠️  Please pass a positive numeric node index.');
    return;
  }
  if (isNaN(Number(argv.port))) {
    console.error('⚠️  Please enter a numeric port.');
    return;
  }
  if (Number(argv.port) < 1) {
    console.error('⚠️  Please pass a positive numeric port.');
    return;
  }
  if (isNaN(Number(argv.rpcPort))) {
    console.error('⚠️  Please enter a numeric RPC port.');
    return;
  }
  if (Number(argv.rpcPort) < 1) {
    console.error('⚠️  Please pass a positive numeric RPC port.');
    return;
  }
  console.log(`[*] passed node index = ${argv.index}`);

  const basePath: string = `../data/boot${argv.index}`;
  console.log(`[*] node base path = ${basePath}`);

  // build_spec.sh
  shell.exec('scripts/build_spec.sh');
  console.log('[*] build chain spec');

  // main boot node
  if (argv.index === 0) {
    // set_main_node.sh
    shell.exec(`scripts/set_main_node.sh ${basePath}`);
    console.log('[*] set boot node');
  }

  // run_boot_node.sh
  shell.exec(`scripts/run_boot_node.sh ${argv.index} ${argv.port} ${argv.rpcPort} ${basePath}`);
  console.log('[*] boot node initialized');
}

runNode().catch(error => {
  console.error(error);
  process.exit(1);
});
