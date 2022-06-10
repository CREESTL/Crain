#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

/*
An auction contract
*/
#[ink::contract]
mod moneybag {

    use ink_storage::traits::SpreadAllocate;
    use ink_env::debug_println;
    use rand_chacha::ChaCha20Rng;
    use rand_chacha::rand_core::SeedableRng;
    use rand_chacha::rand_core::RngCore;
    use scale::{
        Decode,
        Encode,
    };

    #[ink(storage)]
    #[derive(SpreadAllocate)]
    pub struct Moneybag {
        // Number of members
        size: u32,
        // How many bids have been made
        bids_made: u32,
        // Accounts of members;
        members: ink_prelude::vec::Vec<AccountId>,
        // Bids of members
        // NOTE Does not implement Iterator!
        bids: ink_storage::Mapping<AccountId, Balance>,
    }

    #[ink(event)]
    pub struct BidMade {
        from: Option<AccountId>,
        value: Balance,
    }

    #[ink(event)]
    pub struct MaxBidsReached {}

    #[ink(event)]
    pub struct PrizeTransfered {}

    #[derive(Encode, Decode, Debug, PartialEq, Eq, Copy, Clone)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        MaxBids,
        DuplicateMember,
    }

    impl Moneybag {
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
        pub fn add_bid(&mut self, amount: Balance) -> Result <(), Error> {
            if self.bids_made == self.size {
                // Emit the event for log first
                self.env().emit_event(MaxBidsReached{});
                // Give the prize to the winner
                self.give_prize();
                // Return error and do not continue
                return Err(Error::MaxBids);
            }
            let caller = self.env().caller();
            // Same member can't bid more then once
            if self.members.contains(&caller) {
                return Err(Error::DuplicateMember);
            }
            self.bids.insert(&caller, &amount);
            self.members.push(caller);
            self.bids_made += 1;
            self.env().emit_event(BidMade{
                from: Some(self.env().caller()),
                value: amount
            });
            Ok(())
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
            self.env().emit_event(PrizeTransfered {});

        }

        // Function generated a random number of winner
        fn find_winner(&self) -> AccountId {
            // Use AccountId of the caller to add more randomness
            let winner_pos = self.gen_range(0, self.size as u64, self.env().caller());
            let winner = self.members.get(winner_pos as usize).unwrap();
            *winner
        }

        // Function generates a random number in a given range
        fn gen_range(&self, min: u64, max: u64, user_account: AccountId) -> u32 {
            let random_seed = self.env().random(user_account.as_ref());
            let mut seed_converted: [u8; 32] = Default::default();
            seed_converted.copy_from_slice(random_seed.0.as_ref());
            let mut rng = ChaCha20Rng::from_seed(seed_converted);
            ((rng.next_u64() / u64::MAX) * (max - min) + min) as u32
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

    #[cfg(test)]
    mod tests {
        use super::*;

        use ink_lang as ink;

        /// We test if the default constructor does its job.
        #[ink::test]
        fn new_works() {
            let flipper = Flipper::default();
            assert_eq!(flipper.get(), false);
        }
    }

    // TODO add tests here
}
