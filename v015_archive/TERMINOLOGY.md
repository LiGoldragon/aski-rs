# Terminology Note

This code predates v0.17. It uses "domain" to mean "enum only."

In v0.17, **domain** means any data definition (enum + struct +
newtype). The `()` form is called **enum**, not "domain."

Do not carry this old terminology into new code.
