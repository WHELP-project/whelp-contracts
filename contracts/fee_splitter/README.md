# Fee Splitter Contract

The Fee Splitter contract is a smart contract that allows for the distribution of fees to multiple addresses based on predefined weights.

---

## InstantiateMsg

Initializes the contract with 1) List of addresses and their weights. 2) List of cw20 token addresses to check for balance


```json
{
  "addresses": [("user1", 0.5), ("user2", 0.5)]
  "cw20_contracts": [("addr1", "addr2", "addr3")]
}
```

## ExecuteMsg

### `send_tokens`

Transfers tokens send to this contract based on weights from configuration.

```json
{
  "send_tokens": {
    "native_denoms": ["native_addr1", "native_addr2", ...],
    "cw20_addresses": ["addr1", "addr2", ...],
  }
}
```


## QueryMsg

### `config`

Returns the general configuration for the router contract.

```json
{
  "config": {}
}
```