#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

/*
An auction contract
*/
#[ink::contract]
mod moneybag {

    use ink_storage::traits::SpreadAllocate;
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
            // If this function is called after max members was reached
            // return with error
            if self.bids_made == self.size {
                // Emit the event for log first
                self.env().emit_event(MaxBidsReached{});
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
            // If now all 5 members have bids - give someone a prize
            if self.bids_made == self.size {
                self.env().emit_event(MaxBidsReached{});
                self.give_prize();
            }
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
            let winner = self.find_winner();
            if self.env().transfer(winner, for_each.into()).is_err() {
                panic!("Prize transfer failed!");
            }
            self.env().emit_event(PrizeTransfered {});

        }

        // Function generates a random number of winner
        fn find_winner(&self) -> AccountId {
            // Use AccountId of the caller to add more randomness
            let winner_pos = self.gen_range(0, self.size as u64, self.env().caller());
            let winner = self.members.get(winner_pos as usize).unwrap();
            *winner
        }

        // Function generates a random number in a given range
        fn gen_range(&self, min: u64, max: u64, user_account: AccountId) -> u64 {
            let random_seed = self.env().random(user_account.as_ref());
            let mut seed_converted: [u8; 32] = Default::default();
            seed_converted.copy_from_slice(random_seed.0.as_ref());
            let mut rng = ChaCha20Rng::from_seed(seed_converted);
            let res = (rng.next_u64() % (max - min + 1) + min) as u64;
            res
        }

        // Function returns the bid of the caller
        #[ink(message)]
        pub fn get_my_bid(&self) -> Option<Balance> {
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
        use ink_env::test::*;
        use ink_env::DefaultEnvironment;

        #[ink::test]
        fn new_works() {
            let bag = Moneybag::new(100u128);
            assert_eq!(bag.get_bids_made(), 1);
        }

        // Test getters first
        #[ink::test]
        fn get_my_bid_works() {
            let bid = 100u128;
            let mut bag = Moneybag::new(bid);
            let accounts = default_accounts::<DefaultEnvironment>();
            let bob: AccountId = accounts.bob;
            set_caller::<DefaultEnvironment>(bob);
            bag.add_bid(bid).unwrap();
            assert_eq!(bag.get_my_bid().unwrap(), bid);       
        }

        #[ink::test]
        fn get_bids_made_works() {
            let bid = 100u128;
            let mut bag = Moneybag::new(bid);
            let accounts = default_accounts::<DefaultEnvironment>();
            let bob: AccountId = accounts.bob;
            set_caller::<DefaultEnvironment>(bob);
            bag.add_bid(bid).unwrap();
            assert_eq!(bag.get_bids_made(), 2);       
        }


        #[ink::test]
        fn get_total_sum_works() {
            let bid = 100u128;
            // 1 account
            let mut bag = Moneybag::new(bid);
            let accounts = default_accounts::<DefaultEnvironment>();
            // 2 account
            let bob: AccountId = accounts.bob;
            // 3 account
            let charlie: AccountId = accounts.charlie;
            // 4 account
            let django: AccountId = accounts.django;
            for caller in [bob, charlie, django] {
                set_caller::<DefaultEnvironment>(caller);
                bag.add_bid(bid).unwrap();
            }
            assert_eq!(bag.get_total_sum(), bid *  4u128);
           
        } 

        // Test main functions after that
        #[ink::test]
        fn add_bid_works() {
            let bid = 100u128;
            let mut bag = Moneybag::new(bid);
            let init_bids_made = 1;
            let accounts = default_accounts::<DefaultEnvironment>();
            let bob: AccountId = accounts.bob;
            set_caller::<DefaultEnvironment>(bob);
            // Check that Ok is the result
            assert_eq!(bag.add_bid(bid), Ok(()));
            // Check that bids amount increased
            assert_eq!(bag.get_bids_made(), init_bids_made + 1);
            // Check that members contains the bidder
            assert!(bag.members.contains(&bob));
            // Check that bidder has the correct bid amount
            assert_eq!(bag.bids.get(&bob).unwrap(), bid);
        }

        #[ink::test]
        fn gen_range_works() {
            let bid = 100u128;
            let bag = Moneybag::new(bid);
            let acc = AccountId::default();
            let lower = 0u64;
            let higher = 10u64;
            for _ in 0..1000 {
                let number = bag.gen_range(lower, higher, acc);
                assert!(number > lower);
                assert!(number < higher);
            }
        }

        #[ink::test]
        fn find_winner_works() {
            let bid = 100u128;
            let accounts = default_accounts::<DefaultEnvironment>();
            let bob: AccountId = accounts.bob;
            let charlie: AccountId = accounts.charlie;
            let django: AccountId = accounts.django;
            let eve: AccountId = accounts.eve;
            let mut bag = Moneybag::new(bid);

            for user in [bob, charlie, django, eve] {
                bag.bids.insert(&user, &bid);
                bag.members.push(user);
            }

            let winner = bag.find_winner();

            assert!(bag.bids.get(winner).unwrap() == bid);
        }

        #[ink::test]
        fn give_prize_works() {
            let bid = 100u128;
            let accounts = default_accounts::<DefaultEnvironment>();
            let bob: AccountId = accounts.bob;
            let charlie: AccountId = accounts.charlie;
            let django: AccountId = accounts.django;
            let eve: AccountId = accounts.eve;
            let frank: AccountId = accounts.frank;
            let mut initial_balances: Vec<Balance> = Vec::new();
            let mut final_balances: Vec<Balance> = Vec::new();
            for user in [bob, charlie, django, eve, frank] {
                let each_balance = ink_env::test::get_account_balance::<DefaultEnvironment>(user).unwrap();
                initial_balances.push(each_balance);
            }
            let mut bag = Moneybag::new(bid);
            // 5 calls total
            for caller in [bob, charlie, django, eve] {
                set_caller::<DefaultEnvironment>(caller);
                // The fifth call must invoke give_prize()
                bag.add_bid(bid).unwrap();
            }
            for user in [bob, charlie, django, eve] {
                let each_balance = ink_env::test::get_account_balance::<DefaultEnvironment>(user).unwrap();
                final_balances.push(each_balance);
            }

            // Only one user's balance should increase
            let r = final_balances.iter().zip(initial_balances.iter()).filter(|(&a, &b)| a > b).count();
            assert_eq!(r, 1);

        }
    }
}
