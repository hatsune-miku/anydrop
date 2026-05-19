using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Windows.Foundation.Collections;

namespace AnyDrop.Data
{
    public class ObservableDictionary<K, V> : Dictionary<K, V>, IObservableMap<K, V>
    {
        public event MapChangedEventHandler<K, V> MapChanged;

        new public V this[K key]
        {
            get => base[key];
            set
            {
                base[key] = value;
                MapChanged?.Invoke(this, new MapChangedEventArgs<K>(
                    CollectionChange.ItemChanged, key));
            }
        }

        new public void Add(K key, V value)
        {
            base.Add(key, value);
            MapChanged?.Invoke(this, new MapChangedEventArgs<K>(
                CollectionChange.ItemInserted, key));
        }

        new public bool Remove(K key)
        {
            bool removed = base.Remove(key);
            if (removed)
            {
                MapChanged?.Invoke(this, new MapChangedEventArgs<K>(
                    CollectionChange.ItemRemoved, key));
            }
            return removed;
        }

        public void TriggerNotifyChanged()
        {
            MapChanged?.Invoke(this, new MapChangedEventArgs<K>(
                CollectionChange.Reset, default));
        }
    }

    public class MapChangedEventArgs<K> : IMapChangedEventArgs<K>
    {
        public CollectionChange _changeType;
        public CollectionChange CollectionChange => _changeType;

        public K _changedKey;
        public K Key => _changedKey;

        public MapChangedEventArgs(CollectionChange changeType, K key)
        {
            _changeType = changeType;
            _changedKey = key;
        }
    }
}
