// Copyright 2018-2020 Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

#[ink::contract]
pub mod uniswap_pair {

    #[cfg(not(feature = "ink-as-dependency"))]
    use ink_storage::{
        collections::HashMap as StorageHashMap,
        lazy::Lazy,
    };

    use erc20::erc20;

    const MINIMUM_LIQUIDITY: Balance = 10**3;

    #[ink(storage)]
    pub struct Uniswap_pair {
        owner:    AccountId,
        token0:   Lazy<erc20>,
        token1:   Lazy<erc20>,

        reserve0: Balance,
        reserve1: Balance,

        /// Total token supply.
        total_supply: Lazy<Balance>,
        /// Mapping from owner to number of owned token.
        balances: StorageHashMap<AccountId, Balance>,
        /// Mapping of the token amount which an account is allowed to withdraw
        /// from another account.
        allowances: StorageHashMap<(AccountId, AccountId), Balance>,
    }

    #[ink(event)]
    pub struct Sync {
        #[ink(topic)]
        reserve0: Balance,
        #[ink(topic)]
        reserve1: Balance,
    }

    #[ink(event)]
    pub struct Mint {
        #[ink(topic)]
        sender: AccountId,
        #[ink(topic)]
        amount0: Balance,
        #[ink(topic)]
        amount1: Balance,
    }

    #[ink(event)]
    pub struct Burn {
        #[ink(topic)]
        sender: AccountId,
        #[ink(topic)]
        amount0: Balance,
        #[ink(topic)]
        amount1: Balance,
        #[ink(topic)]
        to: AccountId,
    }

    #[ink(event)]
    pub struct Swap {
        #[ink(topic)]
        sender: AccountId,
        #[ink(topic)]
        amount0In: Balance,
        #[ink(topic)]
        amount1In: Balance,
        #[ink(topic)]
        amount0Out: Balance,
        #[ink(topic)]
        amount1Out: Balance,
        #[ink(topic)]
        to: AccountId,
    }

    /// Event emitted when a token transfer occurs.
    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        from: Option<AccountId>,
        #[ink(topic)]
        to: Option<AccountId>,
        #[ink(topic)]
        value: Balance,
    }

    /// up to the amount of `value` tokens from `owner`.
    #[ink(event)]
    pub struct Approval {
        #[ink(topic)]
        owner: AccountId,
        #[ink(topic)]
        spender: AccountId,
        #[ink(topic)]
        value: Balance,
    }

    impl Uniswap_pair {
        /// Creates a new uniswap_pair smart contract initialized with the given value.
        #[ink(constructor)]
        pub fn new(token0: AccountId, token1: AccountId) -> Self {
            Self { owner : Self.env().caller(),
                   token0: erc20::new(token0),  
                   token1: erc20::new(token1), 
                   //lp_token: mpa20::new(lp_token),
                   total_supply: Lazy::new(0),
                   balances:StorageHashMap::new(),
                   allowances: StorageHashMap::new(),
            }
        }

        #[ink(message)]
        pub fn mint(&mut self, to: AccountId) {
            let self_account_id = self.env().account_id();

            let balance0 = self.token0.balance_of_or_zero(self_account_id);
            let balance1 = self.token1.balance_of_or_zero(self_account_id);

            let amount0  = balance0 - self.reserve0;
            let amount1  = balance1 - self.reserve1;

            let total_supply = self.lp_token.total_supply();

            let mut liquidity: Balance = 0;

            if total_supply == 0 {
                liquidity = math::sqrt(amount0 * amount1) - MINIMUM_LIQUIDITY;
                _mint(self, self_account_id, MINIMUM_LIQUIDITY)
            } else {
                liquidity = math::min(amount0 * total_supply / self.reserve0, amount1 * total_supply / self.reserve1);
            }

            assert!(liquidity > 0, "Uniswap: INSUFFICIENT_LIQUIDITY_MINTED");

            _mint(self, to, liquidity);
            update(balance0, balance1, self.reserve0, self.reserve1);

            self.env().emit_event(Mint(self.env().caller(), amount0, amount1));           
        }

        #[ink(message)]
        pub fn burn(&mut self, to: AccountId) {

            assert!(self.env().caller() == to, "Uniswap: auth mismatch"); 

            let self_account_id = self.env().account_id();

            let mut balance0 = self.token0.balance_of_or_zero(self_account_id);
            let mut balance1 = self.token1.balance_of_or_zero(self_account_id);

            let liquidity    = self.lp_token.balance_of_or_zero(to);
            let total_supply = self.lp_token.total_supply();

            let amount0 = liquidity * balance0 / total_supply;
            let amount1 = liquidity * balance1 / total_supply;   

            assert!(amount0 > 0 && amount1 > 0, "Uniswap: INSUFFICIENT_LIQUIDITY_BURNED"); 

            _burn(self, to, liquidity);   

            self.token0.transfer_from(self, to, amount0);
            self.token1.transfer_from(self, to, amount1);

            balance0 = self.token0.balance_of_or_zero(self_account_id);
            balance1 = self.token1.balance_of_or_zero(self_account_id);

            update(balance0, balance1, self.reserve0, self.reserve1);

            self.env().emit_event(Burn(self.env().caller(), amount0, amount1, to));       
        }

        #[ink(message)]
        pub fn swap(&mut self, amount0Out: Balance, amount1Out: Balance, to: AccountId) {

            assert!(amount0Out > 0 || amount1Out > 0, "Uniswap: INSUFFICIENT_OUTPUT_AMOUNT"); 
            assert!(amount0Out < self.reserve0 && amount1Out < self.reserve1, "Uniswap: INSUFFICIENT_LIQUIDITY"); 
            //assert!(to != self.token0.get_address() && to != self.token1.get_address(), "Uniswap: INVALID_TO"); 
            let self_account_id = self.env().account_id();

            if amount0Out {
                self.token0.transfer_from(self, to, amount0Out);
            }  

            if amount1Out {
                self.token1.transfer_from(self, to, amount1Out);
            } 

            let mut balance0 = self.token0.balance_of_or_zero(self_account_id);
            let mut balance1 = self.token1.balance_of_or_zero(self_account_id);

            let amount0In = if balance0 > self.reserve0 - amount0Out {
                 balance0 - (self.reserve0 - amount0Out)
            } else {
                0
            };

            let amount1In = if balance1 > self.reserve1 - amount1Out {
                 balance1 - (self.reserve1 - amount1Out)
            } else {
                0
            };

            assert!(amount0In > 0 || amount1Out > 0, "Uniswap: INSUFFICIENT_INPUT_AMOUNT"); 
            { 
                let balance0Adjusted = balance0 * 1000 - amount0In * 3;
                let balance1Adjusted = balance1 * 1000 - amount1In * 3;
                assert!(balance0Adjusted * balance1Adjusted >= self.reserve0 * self.reserve1 * 1000 * 1000, "Uniswap: K"); 
            }

            update(balance0, balance1, self.reserve0, self.reserve1);

            self.env().emit_event(Swap(self.env().caller(), amount0In, amount1In, amount0Out, amount1Out, to));       
        }

        #[ink(message)]
        fn skim(&mut self, to: AccountId){ 
            assert!(self.env().caller() == self.owner, "Uniswap: auth mismatch"); 
            self.token0.transfer_from(self, to, self.token0.balance_of_or_zero(self) - self.reserve0);
            self.token1.transfer_from(self, to, self.token1.balance_of_or_zero(self) - self.reserve1);
        }

        #[ink(message)]
        fn sync(&mut self){ 
            assert!(self.env().caller() == self.owner, "Uniswap: auth mismatch"); 
            update(self.token0.balance_of_or_zero(self), 
                   self.token1.balance_of_or_zero(self), 
                   self.reserve0, 
                   self.reserve1);
        }


        /// Returns the total token supply.
        #[ink(message)]
        pub fn total_supply(&self) -> Balance {
            *self.total_supply
        }

         /// Returns the account balance for the specified `owner`.
        ///
        /// Returns `0` if the account is non-existent.
        #[ink(message)]
        pub fn balance_of(&self, owner: AccountId) -> Balance {
            self.balances.get(&owner).copied().unwrap_or(0)
        }
           
        /// Returns the amount which `spender` is still allowed to withdraw from `owner`.
        ///
        /// Returns `0` if no allowance has been set `0`.
        #[ink(message)]
        pub fn allowance(&self, owner: AccountId, spender: AccountId) -> Balance {
            self.allowances.get(&(owner, spender)).copied().unwrap_or(0)
        }

        /// Transfers `value` amount of tokens from the caller's account to account `to`.
        ///
        /// On success a `Transfer` event is emitted.
        ///
        /// # Errors
        ///
        /// Returns `InsufficientBalance` error if there are not enough tokens on
        /// the caller's account balance.
        #[ink(message)]
        pub fn transfer(&mut self, to: AccountId, value: Balance) -> Result<()> {
            let from = self.env().caller();
            self.transfer_from_to(from, to, value)
        }

        /// Allows `spender` to withdraw from the caller's account multiple times, up to
        /// the `value` amount.
        ///
        /// If this function is called again it overwrites the current allowance with `value`.
        ///
        /// An `Approval` event is emitted.
        #[ink(message)]
        pub fn approve(&mut self, spender: AccountId, value: Balance) -> Result<()> {
            let owner = self.env().caller();
            self.allowances.insert((owner, spender), value);
            self.env().emit_event(Approval {
                owner,
                spender,
                value,
            });
            Ok(())
        }

        /// Transfers `value` tokens on the behalf of `from` to the account `to`.
        ///
        /// This can be used to allow a contract to transfer tokens on ones behalf and/or
        /// to charge fees in sub-currencies, for example.
        ///
        /// On success a `Transfer` event is emitted.
        ///
        /// # Errors
        ///
        /// Returns `InsufficientAllowance` error if there are not enough tokens allowed
        /// for the caller to withdraw from `from`.
        ///
        /// Returns `InsufficientBalance` error if there are not enough tokens on
        /// the the account balance of `from`.
        #[ink(message)]
        pub fn transfer_from(
            &mut self,
            from: AccountId,
            to: AccountId,
            value: Balance,
        ) -> Result<()> {
            let caller = self.env().caller();
            let allowance = self.allowance(from, caller);
            if allowance < value {
                return Err(Error::InsufficientAllowance)
            }
            self.transfer_from_to(from, to, value)?;
            self.allowances.insert((from, caller), allowance - value);
            Ok(())
        }

        /// Transfers `value` amount of tokens from the caller's account to account `to`.
        ///
        /// On success a `Transfer` event is emitted.
        ///
        /// # Errors
        ///
        /// Returns `InsufficientBalance` error if there are not enough tokens on
        /// the caller's account balance.
        fn transfer_from_to(
            &mut self,
            from: AccountId,
            to: AccountId,
            value: Balance,
        ) -> Result<()> {
            let from_balance = self.balance_of(from);
            if from_balance < value {
                return Err(Error::InsufficientBalance)
            }
            self.balances.insert(from, from_balance - value);
            let to_balance = self.balance_of(to);
            self.balances.insert(to, to_balance + value);
            self.env().emit_event(Transfer {
                from: Some(from),
                to: Some(to),
                value,
            });
            Ok(())
        }

        fn _burn(&mut self, to: AddressId, value: Balance) {
            let to_balance = self.balance_of(to);
            self.balances.insert(to, to_balance - value);

            self.total_supply -= value;
        }

        fn _mint(&mut self, to: AddressId, value: Balance) {
            let to_balance = self.balance_of(to);
            self.balances.insert(to, to_balance + value);

            self.total_supply += value;
        }

        fn update(&mut self, balance0:Balance, balance1:Balance, reserve0:Balance, reserve1:`Balance){
            self.reserve0 = balance0;
            self.reserve1 = balance1;

            self.env().emit_event(Sync(self.reserve0, self.reserve1));
        }


    }

    // #[cfg(test)]
    // mod tests {
    //     use super::*;

    //     #[test]
    //     fn default_works() {
    //         let flipper = Flipper::default();
    //         assert_eq!(flipper.get(), false);
    //     }

    //     #[test]
    //     fn it_works() {
    //         let mut flipper = Flipper::new(false);
    //         assert_eq!(flipper.get(), false);
    //         flipper.flip();
    //         assert_eq!(flipper.get(), true);
    //     }
    // }
}
