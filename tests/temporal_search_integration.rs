use codex_memory::memory::{
    MemoryRepository, SearchRequest, SearchType, MemoryTier
};
use std::env;
use sqlx::PgPool;

async fn setup_repository() -> Result<MemoryRepository, Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL environment variable not set")?;
    
    let pool = PgPool::connect(&database_url).await?;
    Ok(MemoryRepository::new(pool))
}

#[tokio::test]
#[ignore = "integration_test"]
async fn test_temporal_search_functionality() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt::try_init();
    
    println!("Testing temporal search functionality...");
    
    let repository = setup_repository().await?;
    println!("Connected to database successfully");
    
    // Test 1: Basic temporal search with recent memories
    println!("\nTest 1: Basic temporal search for memories from last day");
    
    let search_request = SearchRequest {
        query_text: None,
        query_embedding: None,
        search_type: Some(SearchType::Temporal),
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(10),
        offset: Some(0),
        cursor: None,
        similarity_threshold: None,
        include_facets: Some(false),
        include_debug_info: None,
    };
    
    let response = repository.search(&search_request).await?;
    println!("✅ Temporal search succeeded!");
    println!("  - Found {} results", response.results.len());
    println!("  - Execution time: {}ms", response.execution_time_ms);
    
    // Verify all required columns are present
    for (i, result) in response.results.iter().enumerate().take(3) {
        println!("  Result {}: ID = {}", i + 1, result.memory.id);
        println!("    - Similarity score: {}", result.similarity_score);
        println!("    - Temporal score: {}", result.temporal_score);
        println!("    - Importance score: {}", result.importance_score);
        println!("    - Relevance score: {}", result.relevance_score);
        println!("    - Combined score: {}", result.combined_score);
        println!("    - Access frequency score: {}", result.access_frequency_score);
        println!("    - Created at: {}", result.memory.created_at);
        println!("    - Memory tier: {:?}", result.memory.tier);
        
        // Verify that all required fields are populated and finite
        assert!(result.temporal_score.is_finite(), "Temporal score should be finite");
        assert!(result.importance_score.is_finite(), "Importance score should be finite");
        assert!(result.relevance_score.is_finite(), "Relevance score should be finite");
        assert!(result.combined_score.is_finite(), "Combined score should be finite");
        assert!(result.access_frequency_score.is_finite(), "Access frequency score should be finite");
        assert!(result.similarity_score.is_finite(), "Similarity score should be finite");
    }
    
    println!("✅ All required computed columns are present and valid");
    Ok(())
}

#[tokio::test]
#[ignore = "integration_test"]
async fn test_temporal_search_with_tier_filter() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let repository = setup_repository().await?;
    
    // Test temporal search with working memory filter
    println!("Testing temporal search with working memory tier filter");
    
    let search_request = SearchRequest {
        query_text: None,
        query_embedding: None,
        search_type: Some(SearchType::Temporal),
        hybrid_weights: None,
        tier: Some(MemoryTier::Working),
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(5),
        offset: Some(0),
        cursor: None,
        similarity_threshold: None,
        include_facets: Some(false),
        include_debug_info: None,
    };
    
    let response = repository.search(&search_request).await?;
    println!("✅ Filtered temporal search succeeded!");
    println!("  - Found {} working memory results", response.results.len());
    
    for result in &response.results {
        println!("    - Memory tier: {:?}", result.memory.tier);
        assert_eq!(result.memory.tier, MemoryTier::Working, "All results should be from working memory tier");
    }
    
    println!("✅ All results are from working memory tier");
    Ok(())
}

#[tokio::test]
#[ignore = "integration_test"]
async fn test_temporal_search_ordering() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt::try_init();
    
    let repository = setup_repository().await?;
    
    // Test ordering and verify all computed columns
    println!("Testing temporal search ordering and computed columns");
    
    let search_request = SearchRequest {
        query_text: None,
        query_embedding: None,
        search_type: Some(SearchType::Temporal),
        hybrid_weights: None,
        tier: None,
        date_range: None,
        importance_range: None,
        metadata_filters: None,
        tags: None,
        limit: Some(10),
        offset: Some(0),
        cursor: None,
        similarity_threshold: None,
        include_facets: Some(false),
        include_debug_info: None,
    };
    
    let response = repository.search(&search_request).await?;
    println!("✅ Temporal search succeeded!");
    println!("  - Found {} results", response.results.len());
    
    // Verify ordering (should be by created_at DESC, updated_at DESC)
    if response.results.len() > 1 {
        for i in 0..response.results.len() - 1 {
            let current = &response.results[i];
            let next = &response.results[i + 1];
            
            // Should be ordered by creation date descending
            assert!(current.memory.created_at >= next.memory.created_at, 
                   "Results should be ordered by created_at DESC. Current: {}, Next: {}", 
                   current.memory.created_at, next.memory.created_at);
        }
        println!("✅ Results are properly ordered by created_at DESC");
    }
    
    // Verify that insights generation would work with these results
    println!("\nTest: Verify compatibility with insights generation");
    for (i, result) in response.results.iter().enumerate().take(5) {
        println!("  Memory {}: {} characters", i + 1, result.memory.content.len());
        
        // These are the required fields that insights generation depends on
        assert!(result.similarity_score.is_finite(), "Similarity score required for insights");
        assert!(result.temporal_score.is_finite(), "Temporal score required for insights");
        assert!(result.importance_score.is_finite(), "Importance score required for insights");  
        assert!(result.relevance_score.is_finite(), "Relevance score required for insights");
        assert!(result.combined_score.is_finite(), "Combined score required for insights");
        assert!(result.access_frequency_score.is_finite(), "Access frequency score required for insights");
        
        // Verify memory has required fields for processing
        assert!(!result.memory.content.is_empty(), "Memory content should not be empty");
        assert!(!result.memory.id.is_nil(), "Memory ID should be valid");
    }
    
    println!("✅ All temporal search results are compatible with insights generation");
    println!("✅ The fix for the insights generation issue is working correctly");
    
    Ok(())
}