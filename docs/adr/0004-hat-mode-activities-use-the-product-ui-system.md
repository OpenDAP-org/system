# ADR 0004: HAT-mode activities use the product UI system

- **Status:** Accepted
- **Date:** 2026-10-22

## Context

HAT-mode activities are the only activities beyond the product's own ([ADR 0002](0002-system-architecture.md)). They must not break the product-not-OS principle (SYS-017).

## Decision

- HAT-mode activities follow the same product-not-OS principle and **reuse the product UI system** (design system components and templates), like every other activity.
- When a HAT needs something very specific:
  - while dynamic components are not supported, it **contributes the component to the design system** and the registry, where it is reviewed and integrated;
  - once dynamic components are supported, the component may ship **in the HAT's package**.
