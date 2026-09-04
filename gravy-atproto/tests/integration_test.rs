#[cfg(test)]
mod tests {
    use gravy_atproto::HttpRepoWriter;

    #[test]
    fn test_http_repo_writer_creation() {
        let writer = HttpRepoWriter::new("https://pds.example.com", "did:web:example.com");
        let _ = writer;
    }
}
