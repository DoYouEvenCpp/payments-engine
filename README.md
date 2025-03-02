# Transaction engine
A simple, rust based pet project, that handles specific transactions in a specific way

<br>

## Assumptions
There're a few assumptions that were coined while developing this application:
- locked account affects its state quite significantly, namely: `deposit` or `withdrawal` operations on locked accounts are not permitted. Each results in an `Errors::AccountLocked` error
- `withdrawal` operation on an account that contains an insufficient amount of funds, will effect in an `Errors::Insufficient` error
- each financial operation is tested against overflow, if such happens then an `Errors::FundsOverflow` error is created
- `dispute` requires sufficient funds to be available in the account, if not an `Errors::Insufficient` error is raised
- `chargeback`, `dispute`, and `resolve` are account state (i.e. locked/unlocked) agnostic
- every disputed operation might be resolved/chargedbacked only once
- `chargeback` on `withdrawal` simply brings back funds to the overal available sum, while requestes on `deposit` it reduces held amount *and* locks account
- funds (i.e. floating points) in the output are kept with 4 digits of precission for the decimal point

<br>

## Build
Just as with every rust project, you can build it (but also run, and trigger tests, naturally) by using `cargo`, i.e.:<br>
```
cargo build     # builds application
cargo run       # runs application
cargo test      # runs tests
```

## Documentation

There's `cargo doc` generated documentation avialable under `/doc` directory.



## Running
Application takes only one obligatory parameter, `CSV` file path, e.g.:
```
cargo run -- path/to/my/csv/file.csv
```

<br>

## Input
A specifically crafted CSV, comma-separated, header-based. Whitespaces are discarded.<br>Providing an input that doesn't meet these criteria will effect in an empty output.

## Output
Also specifically crafted, a comma-separated, header-based, CSV file.<br>

<br>


## Developer's guide
### 3rd party libraries used
Here is the list of external crates used for the purpose of this application:
- [clap](https://crates.io/crates/clap): CLI arguments list parser
- [csv](https://crates.io/crates/csv): used for CSV reading/writing
- [anyhow](https://crates.io/crates/anyhow): aids handling of error handling
- [serde](https://crates.io/crates/serde): serialisation and deserialisation operations
- [thiserror](https://crates.io/crates/thiserror): enables helpful derive macro used for Error types definition
- [rust_decimal](https://crates.io/crates/rust_decimal): aids usage of floating point numbers
- [rust_decimal_macros](https://crates.io/crates/rust_decimal_macros): delivers useful macros for testing purposes, mostly
<br>


### Tests

There's a bunch of tests in two modules (`src/account.rs`, and `src/transaction_manager.rs`). The first ones are typical UTs that tests whether the implementation of an account works properly, while the second ones are some sort integration tests (not actualy!), that test proper inter-ops between an account and the transaction manager.

### Further steps
Some brief ideas, that _might_ be a good starting point for a list od `TODOs`:
- run fuzz tests over the implementation just to verify whether the application doesn't crash with unexpected input
- export utility structures (e.g. AccountState, CLI's Args) into a separate module(s)
- the main method returns also `Ok` result - it might be a good idea to start propagating errors to the top of the application (currently, all of the errors are just printed to `stderr`)
- measure, and investigate whether usage of async would be a performance booster: the application has been tested with gigabytes sized (~2GBs) input, results were not terrible, but also not great
- provide an overall design overview
- consider redesigning the solution to leverage event-sourcing mechanism: tracking _events_ done over an account with such approach seems as a good idea

### Known issues
Whenever you see a plausible issue that might increase the technical debt, feel free to point it here, so it just won't get forgotten.

