# Full node 2
## Baltathar validator setting
npm run set_session_keys --   --controllerPrivate 0x8075991ce870b93a8870eca0c0f91913d12f47948ca0fd25b49c6fa7cdbeee8b --provider ws://127.0.0.1:9934
npm run join_validator_candidates --   --bond 4000000 --controllerPrivate 0x8075991ce870b93a8870eca0c0f91913d12f47948ca0fd25b49c6fa7cdbeee8b --stashPrivate 0x14398587839d19c6b0a9e32b2dc2588773b1d2cdb504a2165828511fad5241f5 --provider ws://127.0.0.1:9934 --relayerPrivate 0xb1e7904fb3d5e07f9fae641bee11021d5285732db1b8f061b832fd161d31956a
## Charleth validator setting
npm run set_session_keys --   --controllerPrivate 0x0b6e18cafb6ed99687ec547bd28139cafdd2bffe70e6b688025de6b445aa5c5b --provider ws://127.0.0.1:9935
npm run join_validator_candidates --   --bond 4000000 --controllerPrivate 0x0b6e18cafb6ed99687ec547bd28139cafdd2bffe70e6b688025de6b445aa5c5b --stashPrivate 0x50a8e295bae5d5534d682cd115fb39b523d15f03a2ef638757a84ce7bdb4a1e9 --provider ws://127.0.0.1:9935 --relayerPrivate 0xd46cd71fa879377d2bf2801629d28061e99e6a16bec8a104de45f4981763405b

# Basic node 2
## Dorothy validator setting
npm run set_session_keys --   --controllerPrivate 0x39539ab1876910bbf3a223d84a29e28f1cb4e2e456503e7e91ed39b2e7223d68 --provider ws://127.0.0.1:9936
npm run join_validator_candidates --   --bond 2000000 --controllerPrivate 0x39539ab1876910bbf3a223d84a29e28f1cb4e2e456503e7e91ed39b2e7223d68 --stashPrivate 0x8f17e90dbd66b420e8a5b48e1ed62b573fd6e834c03780ae49235c4f66c69f96 --provider ws://127.0.0.1:9936
## Ethan validator setting
npm run set_session_keys --   --controllerPrivate 0x7dce9bc8babb68fec1409be38c8e1a52650206a7ed90ff956ae8a6d15eeaaef4 --provider ws://127.0.0.1:9937
npm run join_validator_candidates --   --bond 2000000 --controllerPrivate 0x7dce9bc8babb68fec1409be38c8e1a52650206a7ed90ff956ae8a6d15eeaaef4 --stashPrivate 0x5a5d020f9f484160e6120ebf9e8495e509e1ec9e6b59d6898a68ce73d1667655 --provider ws://127.0.0.1:9937
