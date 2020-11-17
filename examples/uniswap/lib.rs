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
        lazy::Lazy,
    };

    use erc20::erc20;
    use mpa20::mpa20;

    const MINIMUM_LIQUIDITY: Balance = 10**3;

    #[ink(storage)]
    pub struct Uniswap_pair {
        owner:    AccountId,
        token0:   Lazy<erc20>,
        token1:   Lazy<erc20>,
        lp_token: Lazy<mpa20>,

        reserve0: Balance,
        reserve1: Balance,
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

    impl Uniswap_pair {
        /// Creates a new uniswap_pair smart contract initialized with the given value.
        #[ink(constructor)]
        pub fn new(token0: AccountId, token1: AccountId, lp_token:  AccountId) -> Self {
            Self { owner : Self.env().caller(),
                   token0: erc20::new(token0),  
                   token1: erc20::new(token1), 
                   lp_token: mpa20::new(lp_token),
            }
        }

        #[ink(message)]
        pub fn mint(&mut self, to: AccountId) {
            let balance0 = self.token0.balance_of_or_zero(self);
            let balance1 = self.token1.balance_of_or_zero(self);

            let amount0  = balance0 - self.reserve0;
            let amount1  = balance1 - self.reserve1;

            let total_supply = self.lp_token.total_supply();

            let mut liquidity: Balance = 0;

            if total_supply == 0 {
                liquidity = math::sqrt(amount0 * amount1) - MINIMUM_LIQUIDITY;
                _mint(self, MINIMUM_LIQUIDITY)
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

            let mut balance0 = self.token0.balance_of_or_zero(self);
            let mut balance1 = self.token1.balance_of_or_zero(self);

            let liquidity    = self.lp_token.balance_of_or_zero(to);
            let total_supply = self.lp_token.total_supply();

            let amount0 = liquidity * balance0 / total_supply;
            let amount1 = liquidity * balance1 / total_supply;   

            assert!(amount0 > 0 && amount1 > 0, "Uniswap: INSUFFICIENT_LIQUIDITY_BURNED"); 

            _burn(self, to, liquidity);   

            self.token0.transfer_from(self, to, amount0);
            self.token1.transfer_from(self, to, amount1);

            balance0 = self.token0.balance_of_or_zero(self);
            balance1 = self.token1.balance_of_or_zero(self);

            update(balance0, balance1, self.reserve0, self.reserve1);

            self.env().emit_event(Burn(self.env().caller(), amount0, amount1, to));       
        }

        #[ink(message)]
        pub fn swap(&mut self, amount0Out: Balance, amount1Out: Balance, to: AccountId) {

            assert!(amount0Out > 0 || amount1Out > 0, "Uniswap: INSUFFICIENT_OUTPUT_AMOUNT"); 
            assert!(amount0Out < self.reserve0 && amount1Out < self.reserve1, "Uniswap: INSUFFICIENT_LIQUIDITY"); 
            assert!(to != self.token0.get_address() && to != self.token1.get_address(), "Uniswap: INVALID_TO"); 

            if amount0Out {
                self.token0.transfer_from(self, to, amount0Out);
            }  

            if amount1Out {
                self.token1.transfer_from(self, to, amount1Out);
            } 

            let mut balance0 = self.token0.balance_of_or_zero(self);
            let mut balance1 = self.token1.balance_of_or_zero(self);

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

        fn _burn(&mut self, to: AddressId, value: Balance) {
            self.lp_token.burn(self, to, value);
        }

        fn _mint(&mut self, to: AddressId, value: Balance) {
            self.lp_token.mint(self, to, value);
        }

        fn update(&mut self, balance0:Balance, balance1:Balance, reserve0:Balance, reserve1:Balance){
            self.reserve0 = balance0;
            self.reserve1 = balance1;

            self.env().emit_event(Sync(self.reserve0, self.reserve1));
        }

        fn skim(&mut self, to: AccountId){ 
            assert!(self.env().caller() == self.owner, "Uniswap: auth mismatch"); 
            self.token0.transfer_from(self, to, self.token0.balance_of_or_zero(self) - self.reserve0);
            self.token1.transfer_from(self, to, self.token1.balance_of_or_zero(self) - self.reserve1);
        }

        fn sync(&mut self){ 
            assert!(self.env().caller() == self.owner, "Uniswap: auth mismatch"); 
            update(self.token0.balance_of_or_zero(self), 
                   self.token1.balance_of_or_zero(self), 
                   self.reserve0, 
                   self.reserve1);
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
