name: Feature request
description: Suggest a product idea or improvement
labels: ["enhancement"]
body:
  - type: textarea
    id: problem
    attributes:
      label: What problem would this solve?
      description: The situation or friction you hit, rather than the solution you have in mind. Product behavior is specified in SPEC/PRD.md — tell us where reality falls short of what you need.
    validations:
      required: true
  - type: textarea
    id: proposal
    attributes:
      label: Your proposal
      description: What you would like the app to do instead.
    validations:
      required: true
  - type: textarea
    id: notes
    attributes:
      label: Anything else?
      description: Similar tools, screenshots, prior discussions.
    validations:
      required: false
