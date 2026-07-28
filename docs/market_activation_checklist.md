# Market Activation Checklist

Before activating a market (i.e. creating the first bond) in Credence, administrators must ensure all required risk parameters are configured. Failure to set any of these parameters will result in activation rejection by the contract.

## Required Parameters

Ensure the following parameters have been explicitly set via their respective setters before any bond is created:

- [ ] **Token Configuration**
  - Setter: `initialize`
  - Ensure the underlying asset token is properly set.
- [ ] **Fee Configuration**
  - Setter: `set_fee_config`
  - Sets the fee recipient and protocol fee bps. Note: `0` is a valid fee.
- [ ] **Bronze Threshold**
  - Setter: `set_bronze_threshold`
  - Defines the minimum bond size for Bronze tier.
- [ ] **Silver Threshold**
  - Setter: `set_silver_threshold`
  - Defines the minimum bond size for Silver tier. Must be strictly greater than Bronze.
- [ ] **Gold Threshold**
  - Setter: `set_gold_threshold`
  - Defines the minimum bond size for Gold tier. Must be strictly greater than Silver and within bounds.
- [ ] **Platinum Threshold**
  - Setter: `set_platinum_threshold`
  - Defines the minimum bond size for Platinum tier. Must be strictly greater than Gold and within bounds.
- [ ] **Maximum Leverage**
  - Setter: `set_max_leverage`
  - Configures the maximum allowed leverage ratio for the market.

## Validation and Bounds

During activation (calling `create_bond`), the contract strictly enforces:
1. **Completeness:** None of the above risk parameters are missing or left uninitialized (except where a `0` value is explicitly allowed, such as zero fees).
2. **Ordering:** Bronze < Silver < Gold < Platinum.
3. **Upper Bounds:** Gold and Platinum thresholds, as well as Fee BPS, must not exceed predefined system maximums.
4. **Bond Limits:** The duration of the activated bond must fall within the strictly allowed minimum (1 day) and maximum (365 days) bounds, and the principal amount must be strictly positive.

*Failure to comply with this checklist will result in a reverted transaction.*
