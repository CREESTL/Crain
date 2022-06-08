#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;


/*
A time-limited auction contract
*/
#[ink::contract]
mod postbox {

    use ink_storage::traits::SpreadAllocate;
    use ink_env::DefaultEnvironment;

    #[ink(storage)]
    #[derive(SpreadAllocate)]
    pub struct Postbox {
        // Number of members
        size: u32,
        // Opening time
        open_time: u64,
        // Closing time
        close_time: u64,
        // Bids of members
        bids: ink_storage::Mapping<AccountId, u32>,
    }

    impl Postbox {
        #[ink(constructor)]
        pub fn new(first_bid: u32) -> Self {
        ink_lang::utils::initialize_contract(
                        |contract: &mut Self| {
                            // Max 5 members
                            contract.size = 5;
                            contract.open_time = ink_env::block_timestamp::<DefaultEnvironment>();
                            // Auction lasts 1 hour
                            contract.close_time =ink_env::block_timestamp::<DefaultEnvironment>() + 3600 * 1000;
                            let caller = Self::env().caller();
                            // Initialize it with the first bid on start
                            contract.bids.insert(&caller, &first_bid);
                        }
                    )
        }

        // Function to add a bid
        #[ink(message)]
        pub fn add_bid(&mut self, amount: u32) {
            let caller = self.env().caller();
            self.bids.insert(&caller, &amount);
        }

        // Function returns the bid of the caller
        #[ink(message)]
        pub fn get_bid(&self) -> Option<u32> {
            let caller = self.env().caller();
            self.bids.get(&caller)
        }
    }

    // TODO add tests here
}
