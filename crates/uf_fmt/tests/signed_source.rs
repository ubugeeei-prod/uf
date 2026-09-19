use uf_config::FmtConfig;
use uf_fmt::format_source;

#[test]
fn relay_signed_artifacts_keep_their_signature_and_comment_casts() {
    let source = concat!(
        "/**\r\n * @generated SignedSource<<9260c7c36b7c02cb9f94bd5e4dcca331>>\r\n */\r\n",
        "const node={kind:'Request'};\r\nexport default ((node/*:: as any*/)/*:: as Query<V,D>*/);\r\n",
    );
    let result = format_source(source, &FmtConfig::default()).unwrap();
    assert_eq!(result.output, source);
    assert!(!result.changed);
}

#[test]
fn a_mention_in_application_code_is_still_formatted() {
    let source = "const message='@generated SignedSource<<9260c7c36b7c02cb9f94bd5e4dcca331>>';";
    assert!(
        format_source(source, &FmtConfig::default())
            .unwrap()
            .changed
    );
}
