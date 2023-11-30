# Rust SupaBase SDK

## Overview

Use SupaBase in Rust with no stress & without worrying about low level implementation.

## Usage

To get started simply initialise a SupaBaseClient through SupaBaseClient::new(supabase_url, secret_key). Then start making requests and you're good to go! (Keep in my secret key and service role are synonymous).

Most requests follow the same structure and require the (table name, id | search param, Option<body>)

If you want new features or improvements let me know.