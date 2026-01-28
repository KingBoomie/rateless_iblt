this is a fork of [this rust crate](https://github.com/samWighton/rateless_iblt) for my personal experimentation. I will propbably never publish it on crates.io. one of the goals is to experiment with APIs, so this crate will quickly look quite different from the original and therefore it's probably not the effort to upstream. I also want to experiment with `![nostd]` and microcontroller support. no async support is planned, unless it turns out to be needed for MCU support. 

The main algorithm here implements efficient set reconciliation. 

---

# Rateless Invertible Bloom Lookup Table (RIBLT).

This crate is based on the paper titled 'Practical Rateless Set Reconciliation' authored by Lei Yang, Yossi Gilad, Mohammad Alizadeh.

https://arxiv.org/abs/2402.02668

Please note that this crate does not look for duplicates in the set. Duplicate items cannot be peeled out of the RIBLT.

## Glossary

- Symbol: An item in the set
- CodedSymbol: An element of the RIBLT.
- Peel: The process of removing a symbol from the RIBLT.

## Overview of what this crate gives you


## Hash collision probability

As described by the birthday paradox, the probability of a hash collision is 50% when the number of items in the set is equal to the square root of the possible outcomes. We are using 64-bit hashes, so we should be expecting hash collisions when we are around 4 billion items.

For sets that approach 4 billion items/symbols will require a larger hash.

## General challenges for very large sets

By their definition, Sets can't have duplicates.
When storing a set in memory in rust, a hashset or BTreeSet can be used.
However, when the set is very large, the memory requirements can be prohibitive.

By definition, insertion when an element already exists is a no-op.
Enforcing this behaviour if the set is stored as an unordered list on disk, checking for a duplicate (before insertion) requires a full scan of the list.

Accompanying data structures, such as a regular bloom filter could reduce the need for a full scan.
If the entry is not in the bloom filter, it known to not yet be in the set, so we can insert/append it safely.
If the entry is in the bloom filter, it might be in the set, so we will need to do a full scan.

## Challenges for rapidly changing sets

Considering the use-case of keeping an insert-only set in sync across multiple servers.
It becomes practical to have three mechanisms for sharing data between servers.
1. A full set transfer, for cases when a new server is added or there are massive differences.
2. A streaming gossip mechanism. An insert on one server is broadcast to all other servers.
3. A repair mechanism, that is run periodically to ensure that all servers have the same set.

Assuming that Rateless IBLT is used for the repair mechanism.
Also assuming that the original insert time for each item is known and that the servers have a clock that is roughly in sync.

Because of the constant insertions to different servers, during busy periods, it is unlikely that the sets will be in sync.
This will result in wasteful use of the repair mechanism, as most differences will be resolved by the gossip mechanism.

To solve for this problem, when computing and sharing the Rateless IBLT, we should ignore items that were inserted within a certain time window.

For example, consider a system where 99.999% of items reach all servers within 10 seconds.
Every server could compute the Rateless IBLT on the minute, every minute for all items that were inserted more than 10 seconds ago.
Servers could share the coded symbols from the Rateless IBLT to a number of other servers. With this information, the servers could begin requesting missing items.

The repair mechanism would also handle cases of a network partition. Rateless IBLT would then be used to efficiently reconcile the differences.

## TODO

[ ] new API on top of the implementation    
[ ] fix bugs unconvered by some prop tests    
[ ] ![nostd]    
[ ] performance work    
