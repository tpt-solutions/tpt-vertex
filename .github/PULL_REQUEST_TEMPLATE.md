name: Pull Request

description: Submit changes to TPT Vertex
title: "[<type>]: <short summary>"
labels: []

body:
  - type: markdown
    attributes:
      value: |
        Thanks for contributing to TPT Vertex! Please fill out the checklist below.

  - type: textarea
    id: summary
    attributes:
      label: Summary
      description: What does this PR do and why?
    validations:
      required: true

  - type: textarea
    id: related
    attributes:
      label: Related issues
      placeholder: "Closes #123"

  - type: checkboxes
    id: checklist
    attributes:
      label: Checklist
      options:
        - label: My changes build cleanly (`cargo build` / `npm run build`)
          required: true
        - label: I added/updated tests where relevant
          required: true
        - label: I ran the linters (`cargo clippy`, `eslint`, `prettier`, `cargo fmt`)
          required: true
        - label: My code follows the project's coding standards
          required: true
        - label: I updated documentation where needed
          required: false
        - label: I agree my contribution is licensed under MIT OR Apache-2.0
          required: true
