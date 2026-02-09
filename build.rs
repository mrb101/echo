fn main() {
    glib_build_tools::compile_resources(
        &["data/resources"],
        "data/resources/echo.gresource.xml",
        "echo.gresource",
    );
}
