# Spadebox &emsp; [![crates.io version]][crates-io] [![NPM version]][npm]

[crates-io]: https://crates.io/crates/spadebox-core
[npm]: https://www.npmjs.com/package/@spadebox/spadebox
[crates.io version]: https://img.shields.io/crates/v/spadebox-core
[NPM version]: https://img.shields.io/npm/v/%40spadebox%2Fspadebox


<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/user-attachments/assets/2f832204-4edc-477a-aec5-f5268aea4756">
    <source media="(prefers-color-scheme: light)" srcset="https://github.com/user-attachments/assets/d0611572-8b35-4d7e-9be1-b1143af419e1">
    <img src="https://github.com/user-attachments/assets/d0611572-8b35-4d7e-9be1-b1143af419e1" width="200px" alt="The Spadebox logo"/>
  </picture>
</div>
<br/>

Spadebox is a set of common tools for AI agents, written in Rust with JavaScript bindings.

Currently, Spadebox includes the following tools:
- `read_file`
- `write_file`
- `edit_file`
- `grep`
- `glob`

Spadebox uses the [`cap-std` crate](https://github.com/bytecodealliance/cap-std) for file system sandboxing.
