# frozen_string_literal: true

require_relative '../lib/example'

RSpec.describe Example do
  it 'adds' do
    expect(Example.add(2, 3)).to eq(5)
  end
end
