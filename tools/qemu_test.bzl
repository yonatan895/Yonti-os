"""Custom Bazel rule for running bare-metal kernel tests in QEMU.

Wraps `//tools:qemu_runner` to create a Bazel test target that:
1. Takes a kernel ELF as a data dependency
2. Invokes qemu_runner to create a bootable image and run QEMU
3. Maps QEMU exit codes to Bazel test pass/fail (33 = success, 35 = fail)
"""

def qemu_kernel_test(name, kernel, timeout = "long", size = "medium", **kwargs):
    """Define a QEMU kernel test.

    Args:
        name: test target name
        kernel: label for the kernel ELF (rust_binary target)
        timeout: Bazel test timeout ("short", "moderate", "long", "eternal")
        size: Bazel test size ("small", "medium", "large", "enormous")
        **kwargs: additional args passed to sh_test
    """
    native.sh_test(
        name = name,
        srcs = ["//tools:qemu_runner"],
        args = ["$(rootpath {})".format(kernel)],
        data = [
            kernel,
            "//tools:qemu_runner",
        ],
        timeout = timeout,
        size = size,
        # QEMU needs to be on PATH
        **kwargs
    )
