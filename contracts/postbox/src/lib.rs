#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

/*
An auction contract
*/
#[ink::contract]
mod postbox {

    use ink_storage::traits::SpreadAllocate;
    use ink_env::debug_println;
    use rand::Rng;

    #[ink(storage)]
    #[derive(SpreadAllocate)]
    pub struct Postbox {
        // Number of members
        size: u32,
        // How many bids have been made
        bids_made: u32,
        // Accounts of members;
        members: Vec<AccountId>,
        // Bids of members
        // NOTE Does not implement Iterator!
        bids: ink_storage::Mapping<AccountId, Balance>,
    }

    #[ink(event)]
    pub struct BidMade {
        from: Option<AccountId>,
        value: Balance,
    }
    impl Postbox {
        #[ink(constructor)]
        pub fn new(first_bid: Balance) -> Self {

        debug_println!("Initializing contract with {} tokens", first_bid);
        ink_lang::utils::initialize_contract(
                        |contract: &mut Self| {
                            // Max 5 members
                            contract.size = 5;
                            // One bid is done in start
                            contract.bids_made = 1;
                            let caller = Self::env().caller();
                            // Add caller to the members
                            contract.members.push(caller);
                            // Initialize it with the first bid on start
                            contract.bids.insert(&caller, &first_bid);
                        }
                    )
        }

        // Function to add a bid
        #[ink(message)]
        #[ink(payable)]
        pub fn add_bid(&mut self, amount: Balance) {
            let caller = self.env().caller();
            self.bids.insert(&caller, &amount);
            self.members.push(caller);
            self.bids_made += 1;
            self.env().emit_event(BidMade{
                from: Some(self.env().caller()),
                value: amount
            });
            if self.bids_made == self.size {
               self.give_prize();
            }

        }

        // Function returns the total sum of bids
        fn get_total_sum(&self) -> Balance {
            let mut total: Balance = 0;
            for member in self.members.iter() {
                let bid = self.bids.get(member).unwrap();
                total += bid;
            }

            total
        }

        // Function transfers all funds to the winner
        fn give_prize(&mut self) {
            let total = self.get_total_sum();
            let for_each = total as u32 / self.size;
            debug_println!("for each part is {}", for_each);
            debug_println!("total balance of contract is {}", self.env().balance());
            let winner = self.find_winner();
            if self.env().transfer(winner, for_each.into()).is_err() {
                panic!("Prize transfer failed!");
            }

        }

        // Function generated a random number of winner
        fn find_winner(&self) -> AccountId {
            let mut rng = rand::thread_rng();
            let winner_pos = rng.gen_range(0..self.size + 1);
            let winner = self.members.get(winner_pos as usize).unwrap();
            *winner
        }

        // Function returns the bid of the caller
        #[ink(message)]
        pub fn get_bid(&self) -> Option<Balance> {
            let caller = self.env().caller();
            self.bids.get(&caller)
        }

        // Function returns the number of bids made
        #[ink(message)]
        pub fn get_bids_made(&self) -> u32 {
            self.bids_made
        }

    }

    // TODO add tests here
}
